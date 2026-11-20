# 内核深度进化 14 号：Q95 环形队列 vs F1-3 Vec 存储对拍裁定件——owner 裁定数据补强（回流裁定项专题）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件把 channel.rs 段 2 的「Q95 定容环形 `Box<[Message; CHAN_DEPTH]>`（base）vs F1-3 `ChannelShared.queues: [Vec<Message>; 2]`（overhaul）」列为 owner 裁定项（与 08 号 RIGHT_EXECUTE 同批）——本件补强裁定所需的对拍数据与实施数据：host 微基线实测两形机械差（满队稳态 recv 2.33x、回滚形 7.6x、k=1 持平），逐消费点改接清单（六点三函数一预储），以及三路裁定选项的代价面——**推荐路 C：回流取 overhaul Vec 形（骨架优先），Q95 环形化接受消亡但不删资产（chanface ring 模块判定律 + host 钉板全保留），回流后若 IPC profile 再证 memmove 占比则立单件在 ChannelShared 内重放环形存储**（重放成本实测清单在册，收益上限实测在册，决策推迟到有真机 profile 数据时点）。

## 审计基线（2026-11-20 00:1x 直核）

- base 线 = `origin/wip/sched-1.1@119dc9e46`（Q120 合流后 tip）；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（未前进）；两线 channel.rs 存储形态实读（非转述）。
- base 形存储：`ChannelEnd.queue: Box<[Message; CHAN_DEPTH]>` + `head/qlen`，pop = 队首槽读出+置空+head 步进（O(1)），push_front = head 环形退一+槽写入（O(1)），send 预占尾槽 `tail_slot()` + `push_tail_commit()`（Q98 直拷窗），预储走 `heap::try_box(from_fn)`（3b3 H2 fallible 面）。
- overhaul 形存储：`ChannelShared.inner.queues: [Vec<Message>; 2]`，pop = `queues[side].remove(0)`（O(n) memmove），push_front = `insert(0)`（O(n) memmove），push = `Vec::push`（O(1)），预储 = create 时两端 `try_reserve_exact(CHAN_DEPTH)`。
- 判定律面两线同源：`queue_full_of/queue_readable/push_ok/pop_ok/push_front_ok` 全吃 `len`（chanface 单源），存储形变不影响判据——06 号件「判定律与存储解耦」定谳的物理实证。
- 微基线环境：host aarch64（Apple M 系列），`rustc 1.97.0 -O`，`black_box` 防消解，模型 Message = 440B 定长（与内核 `bytes[256] + handles 8×16 + MsgIntent + MsgBilling` 同量级），三跑取中位，两形校验和逐位对拍相等。

## host 微基线实测（裁定核心数据）

稳态深度 k（队列滞留 k 条，一轮 recv+send）：Vec 形 `remove(0)` 的 memmove 量 = (k−1)×440B。

| k | Vec ns/步 | Ring ns/步 | Vec/Ring | memmove |
|---|---|---|---|---|
| 1 | 24.3 | 25.3 | 0.96x（持平） | 0B |
| 2 | 29.0 | 25.0 | 1.16x | 440B |
| 4 | 38.5 | 26.4 | 1.46x | 1320B |
| 6 | 48.7 | 24.8 | 1.96x | 2200B |
| 8（满队） | 59.8 | 25.6 | **2.34x** | 3080B |

- 回滚形（pop + push_front，recv 失败复原）：Vec 12.3 vs Ring 1.56 ns/步——**7.6x**（`insert(0)` 全量搬 + 环形退一零搬运；回滚路径每次失败 recv 恒付）。
- 读数判读：k=1 两形持平（无 memmove 时 Vec 索引访问略优），差值随 k 线性放大，满队稳态 2.34x——与 02 号件「满队 recv 一次 ~3.1KB memmove」的推断吻合（3080B ≈ 7×440B）。
- 量级判读：差值 ~34ns/步是 host 单核顺序码测得；内核真机语境（持 LeafIrqLock、cache 冷、IPC 预算 <1200 cycle）下 memmove 占比只会更高——host 数据是**收益下界的保守估计**，方向性结论（k 越大环形越优、回滚恒定差 7.6x）不随环境翻转。

## 逐消费点改接清单（若裁定重放环形——路 B 实施账）

`ChannelShared.inner` 内 `[Vec<Message>; 2]` → `[[Message; CHAN_DEPTH]; 2]` + `heads: [usize; 2]` + `lens: [usize; 2]`（或封 RingBuf 结构体），六消费点：

1. `try_pair` 建对（:208-216）：`q0/q1.try_reserve_exact(CHAN_DEPTH)` 预储 → `try_box(from_fn)` 定容分配（3b3 H2 fallible 面镜像，`Vec::new` + reserve 组合退役）。
2. `ChannelEnd::new` 预储（:248-251）：`pre_queue.try_reserve_exact` 同款改。
3. `drain_side`（:333）：`core::mem::take(&mut g.queues[side])` 锁内整队取出 → 逐槽 `core::mem::replace(&mut slots[i], Message::new())` 置空防旧令牌 alias + head/len 清零（Message::Drop 语义逐条保）。
4. `pop_or_peer_state`（:489-491）：`remove(0)` → 队首槽 `mem::replace` 置空 + `head = ring_next_head(head)` + `len −= 1`。
5. `push_to_peer`（:474-478）：`Vec::push` → 尾槽 `ring_tail_of` 写入 + `len += 1`（O(1)→O(1)，序不变）。
6. `push_front`（:531-532）：`insert(0)` → head 环形退一 + 槽写入（O(n)→O(1)）。

附带面：`send_precheck`/`pop_or_peer_state`/`has_message` 等判据面全吃 `len`（chanface 单源）零改；`Message::new()` 置空初值替代 Vec 空——初始化成本从 create 时一次变 per-slot 置空（pop 时点付，且已是 base 形同款机械）；ktest F1-3 自检面（`test_channel_*` 族）消费 `pop_or_peer_state`/`push_to_peer` 公开签名零改——**回流时 ktest 零触碰**。

## 三路裁定选项（owner 裁定件）

- **路 A（取 base 环形，channel.rs 冲突段全取 base 存储臂）**：保 Q95 资产 + host 微基线收益面；代价 = 回流主件（ChannelShared 无环骨架 + Message::Drop + SIGNAL_PEER_CLOSED）与环形存储正交，需在 ChannelShared 形态上重放上表六点（一次到位但增大回流手术面）；且 13 号件已实证 overhaul 侧 ktest F1-3 族消费公开签名，重放后须复跑全族。
- **路 B（取 overhaul Vec 形，Q95 消亡，资产归档）**：回流手术面最小（channel.rs 冲突段全取 overhaul 即可）；代价 = 满队稳态 2.34x/回滚 7.6x 收益放弃 + 02 号机会三刀 a 正式退役 + chanface ring 模块失去 kernel 消费点（host 钉板仍在，判定律仍被 lib.rs 判据族共用）。
- **路 C（推荐）**：回流时取 overhaul Vec 形（骨架优先，手术面最小），**Q95 环形化接受消亡但不删资产**——chanface ring 模块（`ring_ok/ring_slot_of/ring_next_head/ring_advance/ring_tail_of` 纯函数 + Vec 对拍钉板）全保留；本件六点改接清单 + 微基线数据入册后，回流后若 IPC profile（真机）再证 memmove 占比可观，立单件在 ChannelShared 内重放环形存储（改接清单即施工图，成本已实测）。收益决策推迟到有真机数据时点，避免回流窗口为未证实收益扩大手术面。

## 与其他文档的关系

- 裁定项来源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)（channel.rs 段 2 裁定项深化——本件是其数据补强件）。
- 收益论据：[02-IPC快路径-Top5优化机会-v1.0.md](02-IPC快路径-Top5优化机会-v1.0.md)（机会三现状证据段——「满队 recv ~3.1KB memmove」推断的 host 实证）；骨架重放接缝：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)（判定律与存储解耦定谳 + 消费点六接缝图——路 A 重放时与本件六点清单合读）。
- 微基线复现：`rustc -O` 单文件，模型 Message 440B、black_box 防消解、三跑取中位、校验和逐位对拍——脚本随本件施工过程存档于 hermes 缓存（非仓库资产，数据表已固化本件）。
- 沿革：`docs/阶段沿革.md` 六百一十五节（随本批）。
