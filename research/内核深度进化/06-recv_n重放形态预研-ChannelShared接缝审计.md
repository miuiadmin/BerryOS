# 内核深度进化 06 号：recv_n-on-ChannelShared 重放形态预研——Q112 回流义务 3 前置深化

Type: research
Status: done
Updated: 2026-11-19

一句话说明：Q112（05 号件）裁定回流时 W3 义务 3 = 在 overhaul 的 `ChannelShared` 形态上重放 recv_n 批量收（chanface 判定律零改动 + kernel 消费点改接）——本件把该义务从一句话深化为逐接缝施工图：两线通道内核形态直核对拍后，**消费点改造收敛为六个接缝**（pop 形变／disarm 防双重释放／回滚重取锁压回／peer 唤醒键改接／锁序正交性／唤醒分派同构），其中「成功路径显式 `disarm_handles`」是 base 线不存在、重放最易漏的正确性接缝；判定律面（chanface `recvn.rs` 四导出）与通道内部形态完全解耦，零改动复用定谳维持。

## 审计基线（2026-11-19 19:2x 直核）

- base 线 = `origin/wip/sched-1.1@2cf28e47c`：`sys_channel_recv_n`（syscall.rs :3901 起，W3-Q97）+ `recv_consume_one`（:3663，单发/批量语义单源）+ 通道 = `ChannelEnd` 独立 `VecDeque` 无锁形（`front/pop/push_front` 直操作，原子性全靠 PreemptSpan 单核禁切）。
- overhaul 线 = `origin/wip/base-overhaul@492d5e00c`：通道 = `Arc<ChannelShared>`（`open_ends/closed_once/end_objs` 组合原子态 + `LeafIrqLock<ChannelInner>` 双端队列）+ `Message::Drop` 在途句柄析构 + `pop_or_peer_state` 三态单步弹;`NR_CHANNEL_RECV_N` 在 overhaul 全树零命中（fork 时序缺席，Q112 已定谳非否定）。
- 两线共有件（重放地基）：`recv_rollback`（表 take 回 + 压回队首）、`insert_ref` 令牌入表、Q92 单 PT 锁窗 `_locked` 族、SEC-C 全窗预扫、g3-1c-2 原子段、Q97 A 案唤醒摊销——**全部与通道内部形态正交，原样保留**。

## 消费点六接缝施工图（base 形 → ChannelShared 形）

| # | 接缝 | base 形（Q97 现行） | ChannelShared 形（重放目标） | 风险与裁定 |
|---|---|---|---|---|
| 1 | pop 形变 | `end.front()` 检查 + `end.pop()` 两步（空即断） | `end.pop_or_peer_state()` 单步三态：`Ok(msg)`／`Err(ERR_AGAIN)`（队空且对端开）／`Err(ERR_BAD_STATE)`（对端关） | ERR_AGAIN 即断（poll 语义同形）；ERR_BAD_STATE 也按断处理返回已得 got 条——与 base 线「队空即断」语义兼容（对端关后队列残留消息仍可收，收干即 BAD_STATE 断，无信息损失），批量循环断点从「None」改为「Err 二态」 |
| 2 | disarm 防双重释放 | 无此面（旧 `Message` 无 Drop 析构） | **成功路径必须显式 `msg.disarm_handles()`**（`recv_consume_one` 内 insert_ref 全成 + 出参回写全成之后） | 重放最易漏接缝：overhaul `Message::Drop` 会释放随行句柄 `ObjRef::release`——不 disarm 则消息离开作用域时双重释放；disarm 过早（入表全成前）则回滚路径失去句柄。单发 recv 现行位 = 尾段成功后，批量同位逐条 disarm |
| 3 | 回滚压回 | `recv_rollback` = `end.push_front(msg)` 无锁直压（PreemptSpan 保护） | `push_front` 须在 `LeafIrqLock<ChannelInner>` 临界区内重取锁执行；msg 未 disarm（disarm 只在成功尾）→ 压回消息继续持有句柄所有权，Drop 语义正确 | LeafIrqLock 铁律「临界区内绝不 release/唤醒」天然满足（压回只动队列指针）；第 i 条失败压回后 FIFO 序由锁互斥保证——原 PreemptSpan 对队列面的保护在锁形态下冗余但保留（PT 锁窗/fault 面仍需段守卫） |
| 4 | peer 唤醒键 | `e.peer()` 拿 `NonNull<ObjRef>` 地址做 `wake_matching` 键 | `e.peer_wait_key()`（`end_objs[1-side]` Acquire 原子读 + `peer_is_open` 前置，关侧返 None） | 收尾 WRITABLE 单发（`recv_n_wake_needed(got)` 门）改接此形；键语义等价（end_objs 存的就是 ObjRef 地址）但生命周期由原子态管理，免裸指针互钉论证——SAFETY 注释面反而收窄 |
| 5 | 锁序正交性 | 队列操作在 PreemptSpan + PT 锁窗外段内无锁 | `pop_or_peer_state` 持 LeafIrqLock 极短临界区（弹一条即放），消息所有权移出临界区后续操作全在锁外 | `recv_consume_one` 的 insert_ref（表无锁面）/PT 锁窗/irq_window 拷贝点零触碰 LeafIrqLock——锁序不变式「窗内可达面不新增锁类」保持（LeafIrqLock 在 PT 窗开启前已释放） |
| 6 | 唤醒分派同构 | 直调 `berry_sched::wake_matching` | overhaul `notify_channel_wake` 双态分派：`kl_held()/sq_is_held()` 判 → 直 wake 或 `defer_peer_wake` | recv_n 收尾位点（PT 锁窗外、段收口后）与单发 recv 同构，直接套用同分派；批量窗内恒免唤醒（Q97 A 案）不变——分派只发生在收尾单发一次 |

## 中途失败语义保持（Q97 逐位保持义务）

第 i 条 `recv_consume_one` Err = 该条回滚压回（接缝 3 形）+ 前 i-1 条已消费不回滚（intent 已结算、disarm 已做）+ `*n_recv` = i-1 + 错误码——与 base 线完全一致；唯一形态增量 = 压回动作从无锁直压变锁内压回，可观察行为（FIFO 序、消息原样）零漂移。出参写失败路径（消息已消费、调用方重试态自理）同政。

## 验收义务（回流落地时）

1. base 线 ktest recv_n 全量用例（Q97 系）在 ChannelShared 形态重跑绿。
2. 新增：批量收第 i 条 fault 回滚压回后并发 send 的 FIFO 序保持用例（锁形态专项）。
3. 新增：成功路径 disarm 后消息 drop 无双重释放（可用 ktest 句柄计数对拍）。
4. ERR_BAD_STATE 断路用例（对端关 + 队列残留收干）。

## 与其他文档的关系

- 义务来源（回流总图）：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)——本件深化其义务清单第 3 条。
- 判定律单源：`chanface/src/recvn.rs`（`RECV_N_MAX/RECV_N_SLOT_STRIDE/recv_n_k_legal/recv_n_slot_legal/recv_n_wake_needed`——四导出与通道内部形态零耦合，本件定谳零改动复用的依据）。
- 线体证据：`git show origin/wip/base-overhaul:kernel/src/channel.rs`（ChannelShared/LeafIrqLock/Message::Drop/pop_or_peer_state/peer_wait_key）与 `kernel/src/syscall.rs`（recv_rollback :3557／send 失败臂 disarm :3495）；base 线 `kernel/src/syscall.rs`（sys_channel_recv_n :3901／recv_consume_one :3663）。
- 沿革：`docs/阶段沿革.md` 五百九十六节（随本批）。
