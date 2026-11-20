# 内核深度进化 25 号：dpg.rs 两段冲突处置与中段回退面调和刀 diff 草案——调和面 F 手动解锁残留清单定谳（25 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：22 号件（vmar 十段）/23 号件（PXN/UXN 实装）/24 号件（aspace 两段）后，本件把回流第 1 步随行件 kernel/src/dpg.rs 两段冲突（merge-file 实测 @:149 `probe` KL 入口与根槽重查 / @:287 `probe_el0` 同构段）与其**中段自动并区回退面**升为逐段 diff 草案，并带出**调和面 F（事故族新形态）**：13 号「dpg.rs 2 段取 overhaul（F0-3 IrqSaveGuard 形 + demand_aspace 改接）」裁定对**两段本身**成立，但 fceb→base 线的中段演进（7f05be12e 系把 RAII guard 换回手动 `klock_enter/klock_exit` 对）与 overhaul 段区的 RAII 形在 git 三方合并下**同文件锁纪律二形撕裂**——合并树 `827fef355` 实测：两段冲突区自动落 ours 手动 exit 残留（段区 `<<<<<<<` 内），且中段自动并区带 **5 处 `hal::kern_lock::klock_exit()` 裸调用残留**（merged :161/:203/:297/:325/:450）叠加段区 RAII 入口后 = 每命中路径双解锁（`svc_exit_class(ALL)` + `irq_restore` 两度执行），是比 E0425 更危险的**静默错**（无编译红，DAIF 提前解蔽 = eret 前嵌套 IRQ 踩踏 ELR/SPSR 窗竞态——F0-3 所修的原病灶）。dpg.rs 施工单从「2 段取 overhaul」升为「2 段取 overhaul + 5 处中段解锁残留调和刀 + 供体核活」，且该判普适用于**所有 RAII 化段区的同文件中段**（aspace/vmar/ktest 同查）。

## 审计基线（2026-11-20 07:5x 直核）

- base 线 = `origin/wip/sched-1.1@98ef8cdf4`（#587 W1 合并笔）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 树 `827fef355`（exit=1，十文件冲突面与 13/22/24 号基线零漂）。
- dpg.rs 两段段位 = `git merge-file -p`（ours 1136 行 / base 1128 行 / theirs 1128 行，exit=2）——冲突恰 2 段：`@:149`（probe KL 入口 + 根槽重查）与 `@:287`（probe_el0 同构段），与 13 号「dpg.rs 2 段」裁定零漂。
- 中段残留计数基线：merged 文件 `hal::kern_lock::klock_exit()` ×5 自动并区残留（:161/:203/:297/:325/:450——ours 单边手动形被自动并区带入）；ours 全文件 `klock_exit` ×10 vs fceb/ovh 均 0（RAII 化后无裸 exit）。

## 主发现一：两段冲突解剖——KL 入口二形 + 根槽 API 改名（13 号裁定的实施化）

- **段位 @:149（`probe`）**：ours = base 手动对 `hal::kern_lock::klock_enter()` + miss 臂内联 `klock_exit()`；theirs = overhaul `let _kl = hal::kern_lock::klock_enter_irqsave()`（RAII）+ `vmar::demand_aspace(asid)`（root→aspace 改名）。共用尾区（depth_enter/REQ 武装/wake_matching/block_current）自动并区。
- **段位 @:287（`probe_el0`）**：同构二形（ours 手动 + `demand_root`；theirs RAII + `demand_aspace`）。
- **处置（承 13 号）**：两段全取 overhaul——F0-3 RAII 形 + F1-1 `demand_aspace` 改接一次到位。
- **fceb 侧关键事实**：fceb 的 dpg.rs **已是 RAII 形**（`_kl = klock_enter_irqsave()` :149 / `kl = klock_enter_irqsave()` :277 + `drop(_kl)` :196 / `drop(kl)` :439）——即 13 号裁定段区的「取 overhaul」在 fceb 基线上是**恒等恢复**，非新引入；ours 相对 fceb 的 diff 全量 = RAII→手动对的回退（7f05be12e 系事故）+ `demand_root` 改名 + ZFOD 臂供给 API 回退 + pager 供给链回退 + F6b 判据变量演进（ours 独有，保留）。

## 主发现二：调和面 F（事故族新形态）——中段自动并区锁纪律撕裂，静默错

两段手工取 overhaul 后，**中段自动并区是本件主发现**：ours 侧中段（stage-1 快查后至函数尾）相对 fceb 的单边演进被 git 自动并区带入合并树，其中包含手动锁对的 miss 臂内联 `klock_exit`——与段区 RAII 入口拼装后同函数内两形并存：

| 残留位（merged 行号） | 所属函数/臂 | ours 演进来源 | fceb 原形 | 拼装后果 |
|---|---|---|---|---|
| :161 | `probe` demand_root None 臂 | 手动对回退 | RAII 提前 return（零语句） | 双解锁 ① |
| :203 | `probe` 尾部醒后收口 | `klock_exit()` 替 `drop(_kl)` | `drop(_kl)` | 双解锁 ②（每命中路径必经） |
| :297 | `probe_el0` demand_root None 臂 | 同 :161 | 同 | 双解锁 ③ |
| :325 | `probe_el0` ZFOD 快路径 Ok 臂 | 手动 exit + 丢 `sched_exit_check` | `drop(kl)` + `hal::tlbshoot::sched_exit_check()` | 双解锁 ④ + TLB PENDING 闩排干丢失 |
| :450 | `probe_el0` 尾部收口 | `klock_exit()` 替 `drop(kl)` | `drop(kl)` | 双解锁 ⑤ |

- **危害定谳**：RAII guard 仍存活至函数尾 drop（`svc_exit_class(ALL)` + `irq_restore(saved_daif)`），中段裸 `klock_exit()`（`svc_exit_class(ALL)` + `enable_irq()`）先执行一次 = **每处双执行**：SVC 类位记账双退（`kern_release` 双调——TABLE 位第二次 release 走 held==0 早退无害但记账污染）+ **DAIF 双解蔽**（`enable_irq()` 无条件清 I 位后 `irq_restore` 又恢复入口快照——恢复序侥幸正确，但中间窗 PSTATE.I=0 = F0-3 所修的「eret 前 ELR/SPSR 暴露嵌套 IRQ 踩踏窗」病灶**原样复发**）。无编译红、无 panic——**静默错**，与 24 号 syscall 取号窗泄漏同档，比 E0425 更危险。
- **:325 复合债**：ours 回退把 fceb ZFOD Ok 臂的 `drop(kl); hal::tlbshoot::sched_exit_check();`（F0-3：PT 锁窗内广播的 shootdown 在零锁窗口排干本核 PENDING 闩）换成了裸 `klock_exit()`——供给直供路径的闩排干丢失（跨核对飞账可能滞留至下一窗）。
- **判普（调和面 F 普适律）**：凡「段区手工取 overhaul 引入 RAII 形」的文件，**同文件中段凡 ours 相对 fceb 有手动锁对回退的自动并区，必带裸 exit 残留**——aspace.rs（24 号两段同查）、ktest.rs（第 4 步）、vmar.rs（22 号十段）施工单均须加「中段 `klock_exit`/`kern_release` 残留扫描刀」：`grep -n "kern_lock::klock_exit\|kern_release" <file>` 在 RAII 化文件上应恒零（ovh 终态口径）。
- **与既有判据的关系**：22 号「供体先行原则扩至冲突段手工解引入的消费面」（第八连）管的是**跨文件**供体-消费者链；调和面 F 管的是**同文件内**段区与自动并区的锁纪律一致性——两者正交，合起来才是完整施工判据组。

## 主发现三：供体链核活清单（dpg.rs 面专属三触点 + 一处已活）

- **kern_lock 双 guard（死）**：合并树 kern_lock.rs 自动取 base 形——`klock_enter_irqsave`/`KlockIrqSaveGuard` 零定义（18 号清单首项），段区取 overhaul 即 E0425。复活刀 = 20 号件隔离树已验证的 fceb 形贴回（kern_lock.rs :297-315 双 guard，+55 行）。
- **`tlbshoot::sched_exit_check`（活，降级为直接消费）**：base tlbshoot.rs :193 `pub(crate) fn sched_exit_check()` 在树（18 号清单的 `test_inline_consume_once` 才是回退面，本件不涉）——:325 调和刀恢复消费即编译绿，零供体刀。**注意可见性**：base 形是 `pub(crate)` 而 fceb 形是 `pub`——dpg 同 crate 消费不受影响。
- **`vmar::demand_aspace`（活）**：合并树 vmar.rs :853 自动并区带入（fceb/ovh 同名），段区改接零供体刀。
- **`register_demand_aspace`（活）**：合并树 dpg.rs :1100/:1134 自动取 ovh 形（vmar.rs :844 定义在树）——**boot 自验位零手工刀**（13 号施工图未覆盖此面，本件补登：dpg.rs 尾部 register 位自动并区取对侧，无需列刀）。
- **跨文件静默残留预警（第 10 连候选）**：dpg.rs 整取后 `demand_root` 全文件清零，但 base 侧 ktest.rs ×25 / syscall.rs ×2（:1174/:1562）`demand_root` 消费仍在（合并树 ktest 仅剩 2 处注释残留）——ktest 第 4 步整取时同刀换 `demand_aspace`，syscall :1562 属 08 号段 4 改接面（`new_root_global` 取号窗同批）；**漏改 = E0425（ktest）或静默错（syscall 若只改名不改编译也红——`demand_root` 定义随 vmar 整取消失，两处均编译红，无静默面）**。

## 复活施工卡（dpg.rs 面实施化，三刀一批）

- **刀① 两段取 overhaul**：`@:149` 换 RAII 入口 + `demand_aspace`；`@:287` 同构。fceb 基线恒等恢复背书。
- **刀② 中段调和五处**（调和面 F 本体）：`:161`/`:297` 删裸 exit（RAII 提前 return 零语句）；`:203`/`:450` 裸 exit → `drop(_kl)`/`drop(kl)`；`:325` 裸 exit → `drop(kl); hal::tlbshoot::sched_exit_check();`。终态执法：全文件 `grep -c "kern_lock::klock_exit"` == 0。
- **刀③ 供体核活**：kern_lock 双 guard（20 号 fceb 形贴回）；`sched_exit_check`/`demand_aspace`/`register_demand_aspace` 已活零刀。ZFOD/pager 供给链 API（`supply_page_global`/`supply_page_init_global`）随 vmar.rs 十段（22 号）自动带回，本件不重复列刀。
- 验证口径：`cargo build -p berry-kernel` 0 error + 对照红 = 刀②剥除复现（双解锁无编译红，须靠 review 执法 grep 恒零 + lockdep `--features lockdep` 下 SVC 类位双退的 violation 探针）；QEMU smoke dpg 自验面（boot demand 链 + dpgsoak）。
- 台账随刀：dpg.rs unsafe 面行号漂移重产；沿革让号。

## 与其他文档的关系

- 施工单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——「dpg.rs 2 段取 overhaul」裁定的实施化；本件补中段调和刀与供体核活清单，裁定本体不动。
- 判据链：[18-事故族第四面排查定谳-7f05be12e全symbol扫描收口-6文件18symbol配套静默回退与消费链连带丢失.md](18-事故族第四面排查定谳-7f05be12e全symbol扫描收口-6文件18symbol配套静默回退与消费链连带丢失.md)（供体先行）→ [22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md](22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md)（第八连：跨文件消费面）→ 本件（调和面 F：同文件锁纪律撕裂——静默错族）。24 号件（aspace.rs 两段，`24-aspace.rs两段冲突处置与消费改接面diff草案-第九连供体链生死分叉定谳.md`，PR #588 在途）的第九连与本件调和面 F 正交互补——其管跨文件供体链生死，本件管同文件段区/并区一致性。
- 供体验证单源：[20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)——刀③ kern_lock 双 guard 的 fceb 形贴回实施验证。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（树 `827fef355`）；两段段位 = `git show {origin/wip/sched-1.1,fceb03ff3,origin/wip/base-overhaul}:kernel/src/dpg.rs` 三方 `git merge-file -p`（`<<<<<<<` ×2 @:149/:287）；残留位 = merged 文件 `grep -n "kern_lock::klock_exit"`（×5 + 段区 2 处 ours 臂）；fceb RAII 基线 = `git show fceb03ff3:kernel/src/dpg.rs | grep -n "klock_enter_irqsave\|drop(_kl)\|drop(kl)"`。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百三十三节（随本批；六百三十二为 #588 在途占用，让号序承 Q122 判例）。
