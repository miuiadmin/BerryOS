# 内核深度进化 08 号：syscall.rs 逐段对拍清单预备——回流六冲突段施工图

Type: research
Status: done
Updated: 2026-11-19

一句话说明：Q112（05 号件）回流义务 5 判 syscall.rs 为「组合冲突面最大（合并时逐段对拍）」——本件以 `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1` 实跑把该义务落为**六段冲突施工图**：双侧改动量实测（base +649/−298、overhaul +120/−97）、六段逐一提取两侧形状与定谳解法（3 段 overhaul 权威 + 2 段 base 权威 + 1 段组合重放），另钉两处合并自动并区外义务（KLOG 窗缘防御吸收、RIGHT_EXECUTE 权利域裁定）；结论：syscall.rs 回流不是「217 行整面重放」而是**六点定点手术 + 两个域裁定**，recv_n/send 侧的重放已在 06 号件六接缝图单源。

## 审计基线（2026-11-19 21:1x 直核）

- base 线 = `origin/wip/sched-1.1@16dd1160a`（syscall.rs 4622 行）；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（4294 行，未前进）；merge-base = `fceb03ff3`（4271 行）。
- 双侧净改动：base +649/−298（recv_n/Q98 尾槽直拷/Q97 消费单源/Q100 wait_get 折叠/Q101 门控/环读口三刀/v1 恒 RW 映射）；overhaul +120/−97（F1-3 ChannelShared 消费改接/F1-1 demand 登记改接/telemetry 单 op 收形+窗缘防御/权域位域整理）。
- merge-tree 实跑：syscall.rs 冲突 **6 段**（:58/:698/:1205/:1220/:3654/:3983，合并树 d3f2ae24）；全仓冲突 9 文件，syscall.rs 是唯一多段文件。
- merge-base 中 Q97/Q98 锚点零命中——两件均 fork 后落 base，冲突段的重放归属全部在 base 侧（05 号件「fork 后件 overhaul 零吸收」的逐段实证）。

## 六段冲突施工图（段号 = 合并树行号，merge-tree 实跑提取）

### 段 1（:58-68）use 导入位域——并集解

overhaul 增 `RIGHT_EXECUTE`/`SIGNAL_PEER_CLOSED` 导入；base 侧同窗无新增（RIGHT_EXECUTE 消费 = 0、SIGNAL_PEER_CLOSED 消费 = 0 实测）。解法：取 overhaul 侧（权利域全域导入），base 恒 RW 面不依赖被删的 EXECUTE 位。零语义风险。

### 段 2（:698-735）sys_telemetry_open 循环尾——overhaul 窗缘防御吸收进 base 多 op 形

- overhaul 侧：单 op 形（`if op != TELEMETRY_OP_KLOG` 早拒）+ 循环内 `ptr.checked_add(total)` 回卷拒（base 侧同位置是裸 `ptr + total as u64`）。
- base 侧：多 op match 形（KLOG/RINGS/WSRC 环读口三刀，RINGS_SNAP_STAGE 四段装配 + 头最后落笔）。
- 定谳：**函数骨架取 base（多 op 形），KLOG 臂吸收 overhaul 的 `checked_add(total)` 防御**——base KLOG 臂当前裸加（:726 `ptr + total as u64`）是回卷债，回流即清偿点；RINGS/WSRC 臂单发整段拷出（`copy_to_user(el1, ptr, &stage[..cursor])`）无步进加法面，不受影响。此为 05 号义务 5 外新增的自动并区外义务①。

### 段 3（:1205-1213）sys_vmar_map demand 登记改接——overhaul 权威

base `register_demand_root(vmar.asid(), vmar as *mut Vmar)` + `vmar.map_demand(...)`（v1 形）；overhaul `register_demand_aspace(vmar.asid(), vmar.aspace_ptr())` + `vmar.map_demand_obj(...)`（F1-1 AddressSpace 形）。DemandRootError::{Full,OwnerConflict} 错误映射两侧同文（自动并区）。定谳：**整段取 overhaul**——宿主结构随 F1-1 换代（Vmar 降级为句柄投影），base 侧 v1 形无重放价值；base 在本段的增量只有注释（G4-2 注释链 overhaul 已带同源版）。

### 段 4（:1220-1224）sys_vmar_map eager 映射改接——overhaul 权威 + 权利域裁定

base `vmar.map(crate::mm::frames(), vmo, ..., Perm::UserRw)`（v1 恒 RW 映射，sys 层 `rights != RIGHT_READ|RIGHT_WRITE` 拒 + `get_raw` 恒要求 `RIGHT_MAP|RIGHT_READ|RIGHT_WRITE`）；overhaul `vmar.map_obj_global(vo, vmo, req, voff, len, perm)`（perm 按权利位派生，sys 层保留 READ/WRITE/EXECUTE 全域 + W^X 铁律 + MMIO 拒 EXECUTE）。定谳：**调用形取 overhaul，权利域两线语义分叉须 owner 裁**：base 的「v1 恒 RW」是显式收权面（READ 权逃逸修批 3b3 M6 注释自署），overhaul 保留全域位域。若回流采 overhaul 全域形，base 的 M6 收权即被翻回——两形不可静默并存，回流单开裁定。

### 段 5（:3654-3693）sys_channel_send push 臂——组合重放（本件最重段）

- overhaul 侧：`push_to_peer(msg)` 整消息所有权移交（`Result<(), (Message, i32)>` 无损退回）+ 失败臂逐句柄回插表 + `disarm_handles()`——Message::Drop 在途析构防线（F1-3）。
- base 侧：Q98 尾槽直拷（载荷直达尾槽 + 就地槽位构造，440B struct move 全消）+ `push_tail_commit()` 仅 commit（len +1）+ 不可达防御面闭 SQ 窗。
- 定谳：**两者是不同层的优化，必须组合重放**：Q98 的直拷构造在「满检过→commit」窗口内（send_precheck 已过满检、单核禁切无插队），F1-3 的 push_to_peer 是共享锁内二次核验形。重放形 = Q98 直拷保留 + 直拷窗外包 overhaul 的二次核验语义（peer_is_open 双检）+ 失败臂回滚链按 F1-3 所有权形重写（直拷部分构造的尾槽回滚 = Q98 已有的 slot 回退 + F1-3 的句柄回插）。施工顺序：先落 F1-3 骨架后嵌 Q98 直拷窗（06 号件六接缝同族），勿反向（反向 = 在退役的 ChannelEnd::pair 形上重写直拷，浪费一刀）。
- 佐证：合并自动并结果已保留 Q98 三段注释（:3553/:3585/:3681）——非冲突区自动存活，冲突只在 commit 调用形一点。

### 段 6（:3983-4001）sys_channel_recv 收尾唤醒——base 权威 + disarm 义务嵌接

- base 侧：Q97 消费单源（`recv_consume_one` 按值消费，msg 随消费序析构）+ A 案摊销（4-2 WRITABLE 唤醒收尾一次，`peer()` 取原始端点指针作 key）。
- overhaul 侧：`msg.disarm_handles()` 显式解除所有权 + `peer_wait_key()` 取共享键。
- 定谳：**骨架取 base（Q97 单源形），嵌 overhaul 的 disarm 义务**——F1-3 的 Message::Drop 会析构在途句柄，Q97 的按值消费若不 disarm 则 Drop 链双重释放；重放形 = `recv_consume_one` 成功路径尾部（或其内部 settle 段）补 `disarm_handles()`，唤醒键改 `peer_wait_key()`（共享形下原始指针 key 语义已换）。与 06 号件六接缝图接缝②（成功路径显式 disarm）同一条义务，单源在 06 号。

## 合并自动并区外义务（merge-tree 冲突段之外）

1. **KLOG 窗缘防御吸收**（段 2 定谳已含）：base KLOG 臂裸 `ptr + total` 在回流时随段 2 解法清偿；若回流前 base 先行清偿（独立小刀），段 2 冲突面缩为纯 op 形冲突。
2. **RIGHT_EXECUTE 权利域裁定**（段 4 定谳已含）：base v1 恒 RW 收权面 vs overhaul 全域位域——owner 裁定项，回流单开。
3. **task_exit_hook 四 hunk 无冲突自动并**：overhaul 的进程退出钩子（F1-2 两阶段销毁消费点）自动并入，base 侧无同窗改动（Q101 诊断门控在不同函数段）——并入后须跑 ktest F1-2 面回归（2000 子进程生灭帧账）。

## 与其他文档的关系

- 义务来源（回流总图）：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)——本件把其义务 5「逐段 Q 号锚点对拍」落为六段施工图。
- 姊妹件：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)（recv 侧重放六接缝，与段 5/6 同批施工）；[07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md](07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md)（义务 2 裁定）。
- 证据单源：合并树 `d3f2ae24257da5a612c798fbcd8d61dfb3ac07a4`（merge-tree 实跑可复现：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1`）；六段提取与两侧形状对照全文存 W3 工作区 scratch（q115/）。
- 沿革：`docs/阶段沿革.md` 六百零七节（随本批）。
