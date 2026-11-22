# P2-4 SMP 双窗出参执法：not-applicable 裁定（结构性已覆盖）

Status: implemented

## Problem

五十六轮 copy P1 引入 [access_ok_window](../../../kernel/src/syscall.rs)（单 PT 锁窗内逐页校验 + 同页缓存跳过，256 走查 → 16+1 锁）后，R2 安全 Low 把「SMP 双核各持一窗的出参失效面」挂账为路线图 [P2-4](../../../docs/系统优化路线图-v1.0.md)：担忧形态 = core A 在锁窗内走查通过后、store 前或 store 中，core B 撤同页映射（unmap/protect/demand 供给链），内核对已 unmap 页写 = 内核 fault / 越权写。任务：确认拷贝是否完全在锁窗内、他核一切页表修改是否必取同一把锁；若两面均成立则结构性已覆盖，报告 not-applicable。

## Decision

**裁定 not-applicable，零代码改动**——S-4 4-2 立下的 per-AS PT 锁（[vmar.rs](../../../kernel/src/vmar.rs) `PT_LOCKS`，键 = ASID = 表树身份）已把竞态面全数关死。锁互斥面全列：

- **读侧（walk+store 同窗）五物理位点**：`copy_from_user`（syscall.rs:286）/`copy_to_user`（:318）/`copy_in_u64`（:328）各自 `pt_lock_current()` 开窗，walk（`access_ok` → `walk_leaf`，direct map 读表不经 TLB）与 store/read 均在 RAII `PtGuard` 窗内；`sys_vmo_read`（:696）/`sys_vmo_write`（:740）单一 PT 锁窗包整段 256B 块流，每块经 `copy_*_user_locked` 同窗走查+拷贝。同页缓存 `last_page` 是每窗栈局部量（`SAME_PAGE_NONE` 初始化），禁提升 static 的注在席，跨窗零复用。send/recv 局部解蔽段全部经上述 helper，段级禁切保窗内无 AS 切换（§A113）→ 走查键 = 拷贝键 = 窗键（`current_asid` 与 `current_ttbr0_pa` 同一 TTBR0 读点）。
- **写侧（PTE 修改全取同锁）**：用户表树全部 volatile PTE 写点仅五处——`map` 填叶（vmar.rs:362）、`supply_page` 填叶（:469）、`next_table` 中间描述符（:617，只经 `l3_slot_create` ← map/supply_page 两调用方，均在锁内）、`clear_leaves` 清叶（:654，只被 unmap 与 map 回滚臂调）、`protect` 改 AP（:546）；五个写方法（map :308 / map_demand :399 / supply_page :438 / unmap :505 / protect :538）整方法 `pt_lock(self.asid)`——同树写者与读者必撞同槽互斥。锁外改表面为零：`Vmar::Drop` 不写 PTE（v1 表树帧永不回收）；VMO decommit 要求 mapcnt==0（无映射别名可清）；TLB shootdown 收端（tlbshoot.rs `irq_ack`）与 ASID 回卷 quiesce 纯 TLB 失效零表写；内核半（TTBR1）只在 `build_boot_pt`（.text.boot，单核 SMP 前）写——EL1 免锁高半直映路径结构安全。
- **场景裁决**：场景 A（A 窗内拷完才放锁，B 的 unmap 串行在后）安全；场景 B（锁外改表者）穷举不存在，结构性不可达。ASID 回卷同号复用锁槽只会过度串行（无害），世代隔离归 roll_quiesce；表帧永续故锁外 stale 走查（fail-fast 前置 `access_ok` :677/:724，代码注已载「非安全边界」）也只读确定内存。双窗出参模式（`vmo_get_size` 预清+结果、`vmar_map` in-out、`handle_duplicate/replace`、`channel_recv`）：两窗各自持锁新鲜走查，窗间跨核 unmap/protect 由次窗吸收为 -EFAULT，出参值取自内核状态无撕裂面——预清是 fail-clean 语义非安全边界。

## Alternatives considered

先搜（含 rejected/）：关键词「双窗/access_ok_window/PT 锁/TOCTOU/同窗」零相关命中（三篇误中为 VNC 端口 TOCTOU / pwproto 冻结窗 / PI 调度，均无关），无 supersession。

- **修复案一（补齐页表修改路径同锁执法）——否决**：已是现状，五写方法全取锁，无可补之面。
- **修复案二（access_ok_window 窗内加 TLB invalidation broadcast）——否决**：错靶——store 经 stale TLB 译写的前提是窗内走查通过，而能在窗内撤映射的 unmap/protect 已被 PT 锁挡在窗外；broadcast 本就由写者侧窗内发出 + dispatch 尾锁外等回执（tlbshoot §3.2 纪律）。读者窗内发 SGI 反而制造「持 PT 锁等回执」——正是该协议明令禁止的等待形态。
- **加固件（lockdep/static 断言 PTE 写只在锁内）——否决**：lockdep `CLASS_PT` 取放记账已集中在 `pt_lock`/`PtGuard::drop` 两位点覆盖全部六个物理取点；no_std 无静态机制钉「写点必在锁内」，本裁定的穷举清单即审计基线。

## Consequences

- 零代码 diff；路线图 §3/§5 P2-4 行翻「不适用关闭」。
- 裁定承重三事实，后续刀动其一则本裁定须复审：① `last_page` 窗局部性（禁 static 注不得破）；② v1 表树帧永不回收——P2-2 退出异步化若引入表帧回收，回收须在同一 per-AS PT 锁窗内并重审 stale 走查面；③ 新增 PTE 写者/新增用户缓冲拷贝 helper 必须分别取同锁/同窗纪律（五写点 + 五读窗清单为对拍基线）。
