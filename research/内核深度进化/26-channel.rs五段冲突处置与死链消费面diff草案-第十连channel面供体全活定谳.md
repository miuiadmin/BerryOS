# 内核深度进化 26 号：channel.rs 五段冲突处置与死链消费面 diff 草案——第十连 channel 面供体全活定谳（26 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件把 channel.rs 列为「5 段 = 07/06 号施工图实弹（recv_n-on-ChannelShared + 环形队列并存裁定）」但未逐段落刀——本件以三方 merge-file 实跑（ours 324 / fceb 258 / overhaul 540 行，冲突恰 5 段 @:15/:31/:198/:231/:470 与 13 号基线零漂）升为逐段 diff 草案，并带出四个新定谳：①**供体链全活零刀**（LeafIrqLock 定义在本文件 theirs 侧随段 3 自动活；defer_peer_wake/sq_is_held/wait_signal_peer_state 三个跨文件新增供体合并树自动活——24 号「新增件自动活、回退件自动死」判据的 channel 面全活实证，与 aspace 面三死触点形成对照）；②**死链在消费侧**——段 5 取 overhaul 后 base 方法组（front/pop/tail_slot/push_tail_commit/peer）消亡，syscall.rs 自动并区残留消费 ×4 点在六个冲突段全部窗外（:3566/:4087/:4088/:4127），按 08 号六段施工图走必漏 = E0425 ×4；③**08 号段 5「组合重放」裁定须按 14 号路 C 语境修正**——Q98 直拷窗宿主方法（tail_slot/push_tail_commit）随 Vec 形消亡，段 5 从「Q98 直拷保留 + F1-3 二次核验组合」修正为「取 overhaul + Q98 直拷窗剥除 + 单搬运损失入册」；④**AM-1.5 冻结面 overhaul 已自带**（ChannelInner.frozen_owner 双端数组形，13 号未记录）——ours 单端指针形随骨架退役、handle.rs 消费点（mark_frozen/clear_frozen 同名同签名）自动并区绿。

## 审计基线（2026-11-20 08:4x 直核）

- base 线 = `origin/wip/sched-1.1@13efafd02`（#591 合流后 + NET 十轮 sweep tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 树 `0af728bdec`（exit=1，冲突 10 文件与 13/22/24/25 号基线零漂）。
- channel.rs 五段段位 = `git merge-file -p`（ours 324 行 / base 258 行 / theirs 540 行，exit=5）——冲突恰 5 段：`@:15`（use 块）/`@:31`（chanface re-export）/`@:198`（ChannelEnd 结构定义）/`@:231`（try_unpaired/try_pair 构造族）/`@:470`（方法组），与 13 号「channel.rs 5 段」段位基线零漂。
- syscall.rs 六段段位复核（合并树实测）：:58/:698/:1205/:1220/:3654/:3983——08 号施工图零漂；死链消费点位以合并树 `0af728bdec` 行号为准。

## 主发现一：五段解剖（13 号裁定的实施化）

| 段位 | 冲突内容 | ours（base） | theirs（overhaul） | 处置 |
|---|---|---|---|---|
| :15 | use 块 | `crate::handle::{ObjRef, Rights}` + `alloc::vec::Vec` | `alloc::sync::Arc` + `Vec` + `core::cell::UnsafeCell` | 并集（theirs 的 `Rights/ObjRef` 由下方 `use crate::handle` 大块自动带入——ours 顶部窄 use 摘除，查重防 double-import） |
| :31 | chanface re-export | 12 导出含 Q95 ring 族（`ring_next_head/ring_ok/ring_slot_of/ring_tail_of`）+ W3-Q9/Q49 注释 | 7 判据导出 + `wait_signal_peer_state` | 取 overhaul + ring 族剥除（14 号路 C：存储臂消亡、chanface ring 模块判定律 + host 钉板全保留） |
| :198 | ChannelEnd 结构 | `{peer, queue: Box<[Message; CHAN_DEPTH]>, head, qlen, no_intent, frozen_owner}` | `{side, no_intent, shared: Option<Arc<ChannelShared>>, pre_queue}` + 前置 `ChannelShared`/`ChannelInner`/`LeafIrqLock`/`Message::Drop` 全套 | 取 overhaul（F1-3 骨架；frozen 承接见主发现四） |
| :231 | 构造族 | `try_unpaired` 单形（环形预储 `try_box(from_fn)`） | `try_pair` 首选（Arc 一次建对）+ `try_unpaired` 兼容形（pre_queue 预储，`pair` 时 `mem::take` 移入 ChannelShared） | 取 overhaul 双形——ktest ×9 + bench ×1 的 `try_unpaired`/`ChannelEnd::pair` 消费自动绿（theirs 保留 `pub unsafe fn pair` 同签名 :264） |
| :470 | 方法组 | `queue_full/has_message/push/front/pop/push_front`（ring 机械）+ `tail_slot/push_tail_commit`（Q98 直拷窗） | `wait_signals/send_precheck/push_to_peer/pop_or_peer_state/push/push_front`（ChannelShared 形） | 取 overhaul（路 C）；死链消费面见主发现二 |

- 调和面 F 检查（25 号判普执法）：channel.rs 三方 `klock_enter/klock_exit/kern_release` 全零命中——LeafIrqLock 是独立自旋原语非 klock 族，本文件不在调和面 F 面，零调和刀。
- `push_front` 签名差：ours `&mut self` vs theirs `&self`（锁内重取形）——syscall :3760 消费经 `as_channel_mut()` 拿 `&mut` 调 `&self` 方法自动兼容，零改接。

## 主发现二：死链消费面——E0425 ×4 自动并区点（本件主发现）

段 5 取 overhaul 后 base 方法组（front/pop/tail_slot/push_tail_commit/peer/queue_full）消亡，但合并树 syscall.rs/ktest.rs 自动并区仍有消费残留。逐点核位（合并树 `0af728bdec` 行号，syscall 六段窗 = :58/:698/:1205/:1220/:3654-3693/:3983-4001）：

| 消费点 | 合并树位 | 方法 | 窗归属 | 处置归属 |
|---|---|---|---|---|
| Q98 直拷窗首段 | syscall :3566 | `peer_end.tail_slot()` | **六段全部窗外**（自动并区） | 新列刀：剥除随路 C |
| Q98 直拷窗收口 | syscall :3667 | `peer_end.push_tail_commit()` | 段 5（:3654-3693）窗内 | 随段 5 手工解处置 |
| recv_n 批量循环 | syscall :4087/:4088 | `end.front()` + `end.pop()` | **六段全部窗外**（自动并区） | 06 号接缝 1（换 `pop_or_peer_state` 三态）——施工位从 08 号段 5/6 语境修正为「自动并区列刀」 |
| recv_n 收尾唤醒 | syscall :4127 | `e.peer()` | **六段全部窗外**（自动并区） | 06 号接缝 4（换 `peer_wait_key()`）——同上列刀 |
| 单发 recv 回滚 | syscall :3760 | `end.push_front(msg)` | 窗外，但 theirs 同名自带 | 零刀（签名 &self 兼容） |
| 冻结自检 | ktest :5874 | `has_message()` | 窗外，但 theirs 同名自带（theirs :423） | 零刀 |
| PI/PILAT/FAST 族 | ktest ×9 | `try_unpaired`/`pair` | 窗外，但 theirs 双形自带 | 零刀 |

- 危害定级：全部是**显式 E0425 编译红**（方法不存在），非静默错——与 25 号 demand_root 同族；风险不在「错而不知」在「按 08 号六段施工图走则四处漏扫」：08 号把 recv_n 消费点归入段 5/6 施工（:3654/:3983 两窗），实跑证明循环体（:4087）与收尾（:4127）均在窗外自动并区。
- 判普（并入 24 号「新增件自动活、回退件自动死」的消费侧对偶）：**凡冲突段手工取 overhaul 致方法组消亡的文件，其消费侧自动并区（冲突窗外的同名/邻域方法调用）必扫死链**——消费面清单不能只按冲突窗划定，须 `git grep` 全文件消费点对账（本件 syscall :4087/:4127 即窗外语义死链实例）。

## 主发现三：供体核活——channel 面全活零刀（第十连定谳）

四问核（base tip `13efafd02` / 合并树 `0af728bdec`）：

| 供体 | 定义位 | base tip | 合并树 | 判 |
|---|---|---|---|---|
| `LeafIrqLock`/`LeafIrqGuard` | overhaul channel.rs 本文件 :97-176 | 0 | 自动活（段 3 整体取 theirs） | **本文件供体**——零跨文件刀 |
| `defer_peer_wake` | overhaul kernel/src/process.rs | 0 | 自动活（:1 命中） | 新增件自动活（24 号判据） |
| `sq_is_held` | overhaul sched/src/sched.rs | 0 | 自动活（:1 命中） | 同上 |
| `kl_held` | base arch kern_lock.rs | 活 | 活 | F0-3 面已在 base，零刀 |
| `wait_signal_peer_state` | overhaul chanface/src/lib.rs（fceb 0） | 0 | 自动活（chanface :4 命中） | chanface 新增导出自动并区带入 |
| `mark_frozen/clear_frozen` | 两侧 channel.rs 同名同签名 | 活 | 活 | handle.rs 消费点（:498/:519）自动并区绿 |

- 第十连定谳：aspace 面（24 号）是「回退件自动死」三触点 E0425；channel 面是**全活零刀**——供体要么在本文件（LeafIrqLock）、要么是 overhaul 新增件（defer_peer_wake/sq_is_held/wait_signal_peer_state，fceb 无此面故无回退可选、git 自动取活侧）、要么 base 已活（kl_held）。两面对照把 24 号判据补完：**死供体只出自「fceb 有 → overhaul 有 → base 回退」的演进链；fork 后新增件与 base 已活件恒零供体刀**。
- `notify_channel_wake` 双态分派（kl_held/sq_is_held 判 → 直 wake 或 defer_peer_wake）：theirs 本文件 :191，随段 3 后自动并区活——三供体消费闭环自动成立。

## 主发现四：AM-1.5 冻结面承接（13 号未记录补登）＋08 号段 5 衔接债

- **frozen 面 overhaul 已自带**：ours `ChannelEnd.frozen_owner: Option<NonNull<ObjRef>>`（单端指针形，W3-Q49 随刀）vs theirs `ChannelInner.frozen_owner: [Option<NonNull<ObjRef>>; 2]`（双端数组形，锁内共享）——13 号段位表未记录此面。取 overhaul 后单端形随骨架退役，双端数组形承接；两公开方法 `mark_frozen/clear_frozen` 同名同签名（theirs :386/:403），handle.rs 两消费点（freeze/thaw 冻结属主标）自动并区绿，零改接刀。判据面 `mark_frozen_ok/clear_frozen_ok`（chanface 单源）两侧同吃，钉板不动。
- **08 号段 5「组合重放」裁定须按 14 号路 C 修正**：08 号（时点先于 14 号）裁定段 5 =「Q98 直拷保留 + 直拷窗外包 overhaul 二次核验语义」的组合重放——其前提是 Q98 直拷窗宿主（`tail_slot`/`push_tail_commit`，环形存储专属方法）存活。14 号路 C 已裁定取 overhaul Vec 形 + Q95 环形化消亡，宿主方法随之消亡：:3566/:3667 两消费点 = E0425 死链。修正后的段 5 施工 = 取 overhaul `push_to_peer`（共享锁内二次核验 + `Vec::push` 尾进）+ Q98 直拷窗两消费点剥除（用户载荷改为构造后 move 单搬运——损失 = 直拷省去的那次 440B struct move，量级见 14 号微基线「k=1 两形持平」行：无 memmove 时 Vec 索引访问与环形等价，损失仅构造侧一次 move，非热路径量级）；收益回收路径 = 14 号路 C 预留的「回流后 IPC profile 再证则单件重放环形存储」时点，届时直拷窗随六点改接清单（14 号）一并重放。
- ktest F1-3 族（`test_channel_*` 消费 `pop_or_peer_state/push_to_peer` 公开签名）零触碰（14 号已证）；ktest.rs 本体随第 4 步整取自动带回消费面。

## 施工卡（channel.rs 面实施化，三刀一批）

- **刀① 五段取 overhaul**：:15 use 并集（窄 use 查重）；:31 re-export 取 overhaul + ring 四导出剥除；:198/:231/:470 整取 theirs（F1-3 骨架 + 双构造形 + ChannelShared 方法组）。frozen/try_unpaired/pair/has_message/push_front 五同名面自动承接零刀。
- **刀② syscall 死链四点剥除/改接**：:3566 tail_slot 窗剥除（随路 C）；:3667 随段 5 手工解处置；:4087/:4088 front/pop → `pop_or_peer_state` 三态（06 号接缝 1）；:4127 peer() → `peer_wait_key()`（06 号接缝 4）。执法：`git grep -n "tail_slot\|push_tail_commit\|\.front()\|e\.peer()" kernel/src/syscall.rs` 在施工后恒零（.front() 须查非 channel 消费残留）。
- **刀③ 供体零刀确认 + 台账随刀**：供体全活（主发现三）零刀；chanface ring 模块判定律 + host 钉板保留（14 号路 C 资产面）；unsafe 台账 channel.rs 段行号漂移重产；沿革让号。
- 验证口径：`cargo build -p berry-kernel` 0 error（刀②剥除后四死链消）；对照红 = 刀①落而刀②未落时 E0425 ×4 逐字兑现；QEMU smoke channel 自检面 + ktest F1-3 族；handle.rs 冻结面（mark/clear）对拍 ktest 冻结用例随 test-host 全量。

## 与其他文档的关系

- 施工单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——「channel.rs 5 段实弹」裁定的实施化；本件补死链消费面清单与供体核活，裁定本体不动。
- 路线裁定：[14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md](14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md)（路 C 单源——本件段 2/5 处置与 08 号段 5 修正的裁定依据）。
- 接缝图：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)（六接缝单源——刀② :4087/:4127 改接形按接缝 1/4）。
- 施工图修正对象：[08-syscall逐段对拍清单预备-回流六冲突段施工图.md](08-syscall逐段对拍清单预备-回流六冲突段施工图.md)（段 5 组合重放 → 路 C 语境修正 + 段外死链四点补登）。
- 判据链：[24-aspace.rs两段冲突处置与消费改接面diff草案-第九连供体链生死分叉定谳.md](24-aspace.rs两段冲突处置与消费改接面diff草案-第九连供体链生死分叉定谳.md)（第九连「回退件自动死」）→ 本件（第十连 channel 面「全活零刀」+ 消费侧死链对偶判普）；[25-dpg.rs两段冲突处置与中段回退面调和刀diff草案-调和面F静默双解锁定谳.md](25-dpg.rs两段冲突处置与中段回退面调和刀diff草案-调和面F静默双解锁定谳.md)（调和面 F 执法——本文件 klock 零命中豁免实录）。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（树 `0af728bdec`）；五段段位 = 三方 `git show ...:kernel/src/channel.rs` + `git merge-file -p`（`<<<<<<<` ×5）；死链点 = 合并树 `git show 0af728bdec:kernel/src/syscall.rs | grep -n "tail_slot\|push_tail_commit\|\.front()\|e\.peer()"`；供体四问 = `git grep -c` 于 base tip 与合并树各跑一遍。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百三十七节（随本批——原编六百三十五撞 W2-Q245 让号，Q7 契约）。
