# 内核深度进化 28 号：syscall.rs 六段冲突处置与 Q98 直拷窗整面剥除 diff 草案——键型分裂三态定谳（28 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：08 号件把 syscall.rs 落为六段冲突施工图（3 段 overhaul 权威 + 2 段 base 权威 + 1 段组合重放）但段位裁定停在时点先于 14 号路 C——本件以 merge-tree 实跑（合并树随 base 前进漂至 `ae7fef3cd`，六段段位 :58/:698/:1205/:1220/:3654/:3983 与 08 号零漂）升为逐段 diff 草案，并带出三个新定谳：①**send 函数键型分裂三态**——前置解构头（:3504）自动并区已取 theirs 形（`peer_key: usize` + `send_precheck`），而 Q98 直拷中段（:3555-3692）仍以 ours `peer_ptr` 为轴，26 号「两点剥除」实际是**整窗剥除**（tail_slot 取槽 + slot 直拷构造 + meta/billing 槽位直写 + push_tail_commit 收口约 90 行回 theirs 中转形），剥除面不足则 `peer_ptr` 未定义与 `tail_slot` 死链并存；②**KLOG 裸加的窗外义务**——合并树 :666 ours 裸 `ptr + total` 行落在段 2 冲突窗（:698）之外自动并区，段 2 手工解「骨架取 base」时该行须一并触刀换 `checked_add` 形（修复侧窗外义务，26 号判普的消费侧对偶）；③**段 5 裁定按 14 号路 C 终形落定**——08 号「组合重放」正式退役，取 overhaul `push_to_peer` + Q98 直拷窗整面剥除 + 单搬运损失入册（26 号主发现四的实施化落刀面）。

## 审计基线（2026-11-20 09:5x 直核）

- base 线 = `origin/wip/sched-1.1@3142040cb`（#597 合流后）；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`（syscall.rs 4271 行；base 4622 / overhaul 4294）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1` → 树 `ae7fef3cd`（08 号时点树 `d3f2ae24` 随 base 前进漂移，六段段位零漂复核——syscall.rs `<<<<<<<` 恰 6 处 :58/:698/:1205/:1220/:3654/:3983）。
- 三方 merge-file（ours/base/theirs 4622/4271/4294 行）细分 8 段：:3710 并入段 5 尾（push 臂失败回滚链）、:4022 并入段 6 头（Q97 注释 vs disarm 注释）——以 merge-tree 6 段为权威口径（08 号同法）。
- 死链消费点 = 合并树 `ae7fef3cd` 行号口径；与 26 号基线（树 `0af728bdec`）行号漂移 ≤ +5，点位逐一对上（:3566→:3563/:3566 带、:3667→:3692、:4087/:4088→窗内带、:4127→:4127 零漂）。

## 主发现一：六段处置表（08 号裁定 × 14 号路 C × 26 号修正的融合终形）

| 段位 | 冲突内容 | ours（base） | theirs（overhaul） | 处置终形 |
|---|---|---|---|---|
| :58 | use 导入位域 | 无 `RIGHT_EXECUTE`/`SIGNAL_PEER_CLOSED` | 双导入 + `SIGNAL_PEER_CLOSED` | 取 theirs（08 号并集解的本段实例——两符号在 theirs 均有消费，见主发现四） |
| :698 | sys_telemetry_open 循环尾 | 多 op match 形（KLOG/RINGS/WSRC），KLOG 臂裸加 | 单 op 形 + `checked_add(total)` 回卷拒 | 骨架取 base 多 op 形 + KLOG 臂裸加行换 `checked_add`（08 号段 2 定谳；本件补窗外 :666 触刀义务，主发现三） |
| :1205 | sys_vmar_map demand 登记 | `register_demand_root` + `map_demand`（v1 形） | `register_demand_aspace` + `map_demand_obj`（F1-1 形） | 整段取 overhaul（08 号；25 号调和面 F 衔接——`register_demand_root` 调和刀随本段一并落） |
| :1220 | sys_vmar_map eager 映射 | `vmar.map(..., Perm::UserRw)`（v1 恒 RW） | `map_obj_global`（权利位派生 perm） | 调用形取 overhaul；权利域两线分叉维持 owner 裁定项（08 号段 4——回流单开，本件不改裁定） |
| :3654 | sys_channel_send push 臂 | Q98 直拷收口（`push_tail_commit`） | `push_to_peer(msg)` 所有权移交 + 失败臂回插 + `disarm_handles()` | 取 overhaul（14 号路 C 终形；08 号「组合重放」退役——Q98 直拷窗整面剥除见主发现二） |
| :3983 | sys_channel_recv 收尾 | Q97 注释形（无 disarm）+ `e.peer()` 唤醒键 | `msg.disarm_handles()` + `peer_wait_key()` | 骨架取 base + 嵌 disarm 义务 + 唤醒键换 `peer_wait_key()`（08 号段 6 原裁定维持——F1-3 Drop 链双重释放防线） |

- 段 3/4 的 `DemandRootError::{Full,OwnerConflict}` 错误映射两侧同文（自动并区，08 号已录）；25 号 dpg 调和刀与段 3 的 `register_demand_root` 退役同刀窗，施工序随刀 ②（见施工卡）。
- 段 6 窗内 ours 侧 :3999 `e.peer()` 与 theirs 侧 :3988 `peer_wait_key()` 同窗对位——手工解取后者；窗外 recv_n 收尾 :4127 `e.peer()`（26 号死链清单第 4 点）同形换接，单源 06 号接缝 4。

## 主发现二：Q98 直拷窗键型分裂三态——「两点剥除」升「整窗剥除」（本件主发现）

26 号把段 5 处置落为「:3566/:3667 两消费点剥除」，本件逐行核合并树发现剥除面远大于两点——send 函数在合并树呈**三段拼接**态：

1. **头段（:3504-3525，自动并区取 theirs）**：`let (peer_key, peer_empty, peer_frozen)` 三元组解构 + `end.send_precheck()` 单调用形。ours 侧对应块（:3451 `peer_ptr` + `end.peer()` + `peer_end.queue_full()` 四方法链）相对 fceb 该区零改动（Q98 未触前置），theirs 重构自动胜出——**键型已换**：`peer_ptr: NonNull<ObjRef>` → `peer_key: usize`。
2. **中段（:3555-3641，自动并区保 ours）**：Q98 直拷窗本体——`tail_slot()` 取槽 + `copy_from_user_locked` 载荷直达 + `slot.nbytes/handles/nhandles` 直写 + intent/billing 槽位直写（`slot.meta`/`slot.billing.domain_id`/S4-b 区间计费块）。全部以 `peer_end`/`slot`（peer_ptr 轴）为对象——fceb 无此代码（Q98 fork 后新增），新增件自动存活（24 号判据），但其所引 `peer_ptr` 变量已随头段换型而**未定义**。
3. **尾段（:3654-3692，段 5 冲突窗）**：Q98 收口 `push_tail_commit()` vs theirs `push_to_peer` 失败臂回滚链。

- 定谳（剥除面修正）：Q98 直拷窗剥除不是两点替换而是**整窗退役回 theirs 中转形**——中段（约 87 行）整体换回 `Message::new` + `bbuf` 双跳拷贝 + `msg.meta/billing` 中转直写（theirs :3395-3430 形），尾段取段 5 解法。头段零刀（已自动 theirs 形）。**危害对照**：只剥两点则中段 `peer_ptr`/`tail_slot`/`slot` 三重未定义 + 死链；剥整窗则一次对齐 theirs 形。编译红面（E0425 族）在本段从「方法不存在」升级为「变量未定义 + 方法不存在」复合面。
- 损失入册（26 号主发现四口径维持）：直拷省去的 440B struct move 单搬运回来；量级 = 14 号微基线「k=1 两形持平」行——非热路径量级；回收路径 = 14 号路 C 预留的回流后 IPC profile 再证时点。
- 判普（键型分裂，供后续件执法）：**冲突段手工取一侧时，同函数内「头自动取 theirs + 中段保 ours」的拼接态必扫变量轴断裂**——方法死链查 `git grep` 即得，变量未定义查的是「解构头键名 × 消费面变量名」对账（本例 `peer_key` vs `peer_ptr`）。26 号判普「消费侧自动并区必扫死链」补此一刀：**死链清单之外还有键轴清单**。

## 主发现三：KLOG 裸加窗外义务——修复侧的窗外触刀（:666）

合并树 :666 保留 ours KLOG 臂循环内裸加 `match copy_to_user(el1, ptr + total as u64, ...)`，该行落在段 2 冲突窗（:698 起）**之外**自动并区；窗内 theirs 侧 :699-705 才是 `checked_add` 形。成因：ours KLOG 臂（多 op 形三刀之一）相对 fceb 的新增在 theirs 未触的行上自动存活，裸加行恰在此带。

- 段 2 解法触刀面修正：08 号「KLOG 臂吸收 checked_add」不是窗内顺手解——手工解段 2 后**须回窗外 :666 行替换裸加**（`ptr + total as u64` → `checked_add` 回卷拒形），否则回卷债静默残留（不编译红、域外值错写）。
- 判普：**修复侧窗外义务与消费侧窗外死链对偶**——冲突窗手工解只保窗内语义，窗内解法涉及的「同函数窗外行」（本例裸加行）不入刀面则债留；执法 = 施工卡刀 ③ 的施工后 grep 恒零清单把 `ptr + total` 字面量列入。

## 主发现四：段 1 导入消费对账——`SIGNAL_PEER_CLOSED` 三消费点全活

合并树 `SIGNAL_PEER_CLOSED` 命中 3 处：:61（段 1 窗内 theirs 导入）、:2220（task_exit_hook 注释——F1-3 全谱锁窗说明，overhaul 新增件自动并入）、:4191（wait 面 `peer_closed` 臂 `s |= SIGNAL_PEER_CLOSED`——overhaul wait 面新增自动并入）。`RIGHT_EXECUTE` 在 syscall.rs 仅 :61 导入行窗内（其余 11 处消费在 channel.rs/vmar.rs 等文件，随各文件段解）。

- 对账结论：段 1 取 theirs 双导入后 :2220/:4191 两窗外消费即绿，零额外刀；反向若段 1 保 ours（剥 SIGNAL_PEER_CLOSED）则 :4191 编译红——**导入面是消费面的闸门**，段 1 解法唯一（取 theirs），08 号并集解在本段退化为单侧解。
- `RIGHT_EXECUTE` 导入后 syscall.rs 内零消费不红（`pub use` re-export 面非 unused 静默——v1 37 号编号表同窗先例），权域语义分叉维持 08 号段 4 owner 裁定项不动。

## 施工卡（syscall.rs 面实施化，随 26 号三刀批同批——本件为其 syscall 面）

- **刀① 六段手工解**：:58/:1205/:3654/:3983 四段按主发现一处置表落刀；:1220 调用形取 overhaul（权域裁定另开）；:698 骨架取 base。
- **刀② Q98 直拷窗整面剥除**：:3555-3641 中段整体换回 theirs `Message::new` + `bbuf` 中转形（主发现二）；头段 :3504 零刀；尾段随段 5。执法 grep：施工后 `git grep -n 'tail_slot\|push_tail_commit\|peer_ptr' kernel/src/syscall.rs` 恒零（`peer_ptr` 轴全消）。
- **刀③ KLOG 窗外触刀**：:666 裸加行换 `checked_add` 形。执法 grep：`git grep -n 'ptr + total' kernel/src/syscall.rs` 恒零。
- **刀④ 台账随刀**：unsafe 台账 syscall.rs 段行号漂移重产；沿革让号；26 号刀② 的四点死链 grep（tail_slot/push_tail_commit/.front()/e.peer()）并入本卡执法面（同一文件）。
- 验证口径：`cargo build -p berry-kernel` 0 error；对照红 = 仅刀①落而刀②未落时 `peer_ptr` 未定义 + `tail_slot`/`push_tail_commit` E0425 复合红；QEMU smoke IPC 面 + ktest F1-3 族 + telemetry KLOG/RINGS/WSRC 三 op 面随 test-host 全量；right 面走段 4 裁定件后补验。
- 施工序位：在 26 号 channel.rs 三刀之后（channel 面方法组先活，syscall 消费面后接）——顺序反了则 channel 段解完 syscall 全红无意义；与 27 号 ktest 面无序依赖（ktest 消费的是公开签名，两侧同形）。

## 与其他文档的关系

- 施工图基础：[08-syscall逐段对拍清单预备-回流六冲突段施工图.md](08-syscall逐段对拍清单预备-回流六冲突段施工图.md)——本件把其六段裁定落为逐段 diff 草案；其段 5「组合重放」按 14 号路 C 修正、段 2 KLOG 义务补窗外触刀面、段 1 并集解退化为单侧解（本件主发现三/四）。
- 修正依据：[14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md](14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md)（路 C 单源）→ [26-channel.rs五段冲突处置与死链消费面diff草案-第十连channel面供体全活定谳.md](26-channel.rs五段冲突处置与死链消费面diff草案-第十连channel面供体全活定谳.md)（段 5 修正 + 死链四点）——本件「两点剥除升整窗剥除」是 26 号主发现四的实施化落刀面。
- 姊妹件：[27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md](27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md)（ktest 面——十一连序列）；[25-dpg.rs两段冲突处置与中段回退面调和刀diff草案-调和面F静默双解锁定谳.md](25-dpg.rs两段冲突处置与中段回退面调和刀diff草案-调和面F静默双解锁定谳.md)（段 3 调和刀衔接）。
- 接缝图：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)（段 6 disarm/唤醒键 + 窗外 :4127 改接单源——接缝 ②/④）。
- 判据链：24 号（新增件自动活/回退件自动死）→ 26 号（消费侧窗外死链判普）→ 本件（键型分裂三态 + 修复侧窗外义务——两侧窗外义务对偶收口）。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1`（树 `ae7fef3cd`，base tip `3142040cb`）；三方 merge-file = `git show` 三版 kernel/src/syscall.rs + `git merge-file -p --diff3`（`<<<<<<<` ×8 细分粒度，归并 6 段）；键型分裂 = 合并树 `sed -n '3504p;3563p;3692p'`（peer_key 头/peer_ptr 消费/commit 收口三点对拍）；KLOG 裸加 = `git grep -n 'ptr + total'` 于合并树。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百三十九节（随本批——base 现占至六百三十八，Q7 契约顺延）。
