# 内核深度进化 03 号：syscall 分派审计——入口保存·分派机制·快路径·返回恢复 → Top-5 优化机会（影响×可行性）

> 版本：**1.0**｜2026-11-17｜文件名 `03-syscall分派审计.md`
> 定位：内核深度进化四路审计之 03 路（syscall 分派）。必读三件全读：`kernel/src/syscall.rs`（4189 行——头注/install/caller_table/copy 链 1-530 与 4049-4189 直读，handlers 抽样直读）、`kernel/src/main.rs`（7604 行——boot 挂钩序 880-960/2350-2400 直读，spawn 面与 bench 面定点核）、`arch/aarch64/src/`（syscall.rs 88 行全读、trap.rs 734 行全读、kern_lock.rs 398 行全读、tlbshoot.rs 229 行全读、smp.rs 定点、timer.rs 定点、task.rs 走查）；辅证 `sched/src/sched.rs`（preempt_checkpoint/svc_window/current_task/try_preempt 定点）、`svcface/src/lib.rs`（324 行全读——45 号锁类注册表单源）、`ktest.rs`（svc4/svc6 直读）、`tools/bench_baseline_icount.txt`。
> 量化基线：icount 在册六项（`tools/bench_baseline_icount.txt` :14-19，2026-11-15 值：cntvct_pair=0/frame_alloc4k=37/frame_alloc64k=82/handle_evt=274/vmo_commit1p=1683/vmo_rw4k=6948）；全域 50K 行内核、670 unsafe、354 alloc 在册口径。**测量缺口（本路新发现）**：icount 六项全部是内核直调形态（bench.rs :130-175），SVC 分派包络（帧存取+锁类+尾检查）零覆盖；唯 B-1 墙钟通道的 sys_vmo_rw4k 间接受其影响（`tools/bench.py` :268「SVC 入口锁可见代价」自记）。
> 证据纪律：引用 = 仓内路径:行号（2026-11-17 工作树直核，行号随时点漂移，漂移按符号定位）；推断标「推断」；指令开销估算均为 TCG/icount 相对口径（绝对值属真机阶段）。
> 只读审计：未改任何仓内文件；本件为唯一产出，落 `research/内核深度进化/`。禁 git 已遵守。

## 结论速览

一句话判词：BerryOS 的 syscall 分派**骨架设计已经过硬**（表驱动派发零分支瀑布、NONE 类越界号零锁直达、按号锁类位集分档、`-> !` 号舞蹈收口、未知号绝不 panic 的统一错误面），剩余 Top-5 机会全部集中在「**每次调用的恒定税**」——44/45 号实装无差别付：①KERN_LOCK 全局对 + 其上的纯诊断账面（每号一次 CNTVCT 读 + 间接调用 + 5 store）；②dispatch 尾三重固定检查（两次 tlbshoot 闩查各带一次 TPIDR 追链 + 一次 DAIF 三连的抢占检查点）；③入口/出口无条件全量 31 寄存器 288B 帧存取；④纯 copy_out 平凡号（clock_read/signal_take/util_snapshot）被「NONE 归零律」一刀切按在 TABLE 档，而 PT 锁同窗早已接替其 §A113 互斥义务；⑤每号 4 次不可去虚化的间接调用 + 约 5 次 TPIDR 系统寄存器追链。五项均不动 ABI 语义面（机会三 vDSO 为 add-only 扩展）。

## 分派链全景与固定开销拆解（证据基座）

EL0 一次 SVC 的完整路径（EL1 ktest/bench 形态同链，向量槽 0x200 与 0x400 对称）：

1. **入口保存**：向量槽 `b exc_sync_el0`（`arch/aarch64/src/trap.rs` :150）→ `save_frame_el0`（:224-250）：31 个 GPR 16 次存储（15 stp + 1 str，:226-241）+ 5 次系统寄存器读（esr/far/elr/spsr/sp_el0，:242-246）+ 帧头 3 次存储——**19 次内存写 + 5 次 MRS**；`bl exception_sync_el0`（:297-302）→ `ec_of` 分类命中 EC_SVC（:424-429）→ `bl dispatch`。
2. **分派机制**：`dispatch`（`arch/aarch64/src/syscall.rs` :51-88）：栈读 `regs[0]` + 界检（nr ≤ 0xFF）+ 派发表 16B 依赖载入（:59-65，`SYSCALLS` static mut 256 槽 :34-35，boot 期 `install` 填 :42-46）→ `svc_enter_class(class)`（`arch/aarch64/src/kern_lock.rs` :225-233）：先**间接调用** SVC_WINDOW 槽（:226-229，boot 后恒 Some）——钩子体 `berry_sched::svc_window`（`sched/src/sched.rs` :451-486）做 current_task 追链（MRS tpidr + 判空 + volatile 载入）+ 按位物理取结构锁 + `held_locks/svc_mask` 两写；TABLE 位（44/45 号）再 `kern_acquire`（:121-158）= 1 CAS + KL_HELD 写（:149）+ **KL_HELD_SINCE 写（内含一次 `mrs cntvct`，:150）+ NAME_HOOK 间接调用 + 2 写（:153-157）**。
3. **handler 本体**：以最平凡的 0x61 clock_read 为例（`kernel/src/syscall.rs` :429-453）：零 caller_table、2 次 MRS（cntvct/cntfrq）+ 两次 u64 除法 + copy_out（EL0 路自带 per-AS PT 锁同窗走查，:311-319）。
4. **返回恢复**：`svc_exit_class`（kern_lock.rs :239-247）= kern_release 2 写 + 再一次**间接调用** SVC_WINDOW（断言+按位放+记账清）；随后 dispatch 尾三重固定检查：`tlbshoot::sched_exit_check`（`arch/aarch64/src/tlbshoot.rs` :156-161——`cpu_id()` 即 MRS tpidr + volatile 载入（`arch/aarch64/src/smp.rs` :314-321）+ PENDING[me] Acquire 读）→ `roll_quiesce_check`（:195-207——**再来一遍** MRS tpidr + ROLL_PENDING[me] Acquire 读）→ SVC_EXIT_CHECKPOINT **间接调用**（`arch/aarch64/src/syscall.rs` :83-85 → `berry_sched::preempt_checkpoint`，`sched/src/sched.rs` :174-181：MRS daif + MSR daifset + preempt_count 追链（MRS tpidr + 载入）+ MSR daif 恢复——无事可做时也付 3 次系统寄存器写 + 2 读）；最后 `regs[0]` 栈写（:86）→ `load_frame_el0`（trap.rs :252-276）：**19 次内存读 + 3 次 MSR** + eret。

不计 handler 本体，固定包络 ≈ **38 次栈帧存/取 + 约 12 次系统寄存器操作 + 1 次 CAS + 约 10 次全局写 + 4 次间接调用 + 2 次共享行 Acquire 读**。TCG/icount 口径下系统寄存器操作每次走 helper（数十条指令级），是包络大头；其中 kern_acquire 内的 CNTVCT 读与尾查的 DAIF 三连纯属诊断/防御冗余。锁类分布：45 号注册表（`svcface/src/lib.rs` :93-150）中 44 号带 TABLE 位（唯 0x37 thread_sleep 落 SQ_RQ 无 KL，:125）——即**全系统几乎每一号 syscall 都取放全局 KERN_LOCK**（「过渡态 KL-first」自记，`arch/aarch64/src/syscall.rs` :53-55 派发注释 + `arch/aarch64/src/kern_lock.rs` :182-207 槽机制沿革注）。流量画像（推断）：GUI/事件循环负载的高频号为 channel_send/recv（0x52/0x53）、wait_one（0x04）、clock_read（0x61）——全部落在带 KL 的重档。

## Top-5 总表

| 排名 | 机会 | 审计维度 | 影响量级（推断） | 可行性 | 动面 |
|---|---|---|---|---|---|
| 1 | dispatch 尾三重固定检查合并 + kern_acquire 诊断账面退役 | 返回恢复 | 每号省 1 次 CNTVCT 读 + 3 次 MRS tpidr + DAIF 三连 + 1 间接调用 + 5 全局写 | 极高（1a）/中（1b） | kern_lock.rs + tlbshoot.rs + smp.rs + sched.rs |
| 2 | TABLE 档降类复审：0x61/0x87/0x40 落 NONE（PT 锁后继） | 分派机制 | 三热平凡号整对 KERN_LOCK 归零 | 高 | syscall.rs install + svcface + 判例文 |
| 3 | clock_read MONOTONIC vDSO 化（EL0VCTEN 已开） | 快路径 | 整号消灭：入口+锁+尾税全免 | 中 | vmar map 数据页 + userbins/libc |
| 4 | SVC 专用快帧：只存 x0-x18 + x30，帧面减约 40% | 入口保存 | 每号省 22 次帧存取 + 1 MRS | 中低 | trap.rs asm 宏双对 + 两向量槽 |
| 5 | 间接调用/TPIDR 追链一次化 + SYSCALLS 表固化 RO | 分派机制 | 每号省 2-3 次 MRS + 1-2 次间接调用；表得 W^X | 高（5b）/中（5a） | 三 crate 热路径 + mmu |

## 机会一（返回恢复）：尾三重固定检查合并 + kern_acquire 诊断账面退役

**现状证据**。其一，`kern_acquire`（`arch/aarch64/src/kern_lock.rs` :121-158）在 CAS 成功后无条件做三件纯诊断事：KL_HELD_SINCE 写（:150，内含 `timer::cntvct()` 即一次 MRS CNTVCT）、NAME_HOOK 间接调用取任务名（:153-157）、两写 KL_HELD_TAG_PTR/LEN——三者均为 S-6 ③b 自旋看门狗「持者点名」诊断面，代码自记「诊断面，定谳后随探针退役」（:52-63），且 run11 已完成定谳（快速取放活锁归因闭合，:55-59 注）。即**每号 TABLE 类 syscall 都为一次已结案的尸检付一次 CNTVCT 读 + 间接调用 + 3 写**。其二，dispatch 尾三查彼此独立各带一套追链：`sched_exit_check` 与 `roll_quiesce_check`（`arch/aarch64/src/tlbshoot.rs` :156-161/:195-207）各自调 `smp::cpu_id()`（MRS tpidr + volatile 载入，`arch/aarch64/src/smp.rs` :314-321）再 Acquire 读各自的全局 per-核闩数组（PENDING :70 / ROLL_PENDING :80——单核生产面恒 0 仍要读）；SVC_EXIT_CHECKPOINT（`arch/aarch64/src/syscall.rs` :83-85）间接调 `preempt_checkpoint`（`sched/src/sched.rs` :174-181）——无事可做时也付 MRS daif + MSR daifset + MRS tpidr（preempt_count 槽）+ MSR daif 恢复的完整 DAIF 三连。

**优化方案**。1a（独立小刀）：诊断账面移入争用臂——CAS 首败（已进 KL_CONTENDED_CAS 计数支）或 `par_enabled()` + 诊断旗标时才维护 KL_HELD_SINCE/NAME 镜像；快路径只留 CAS + KL_HELD 写（KL_HELD 是 `kl_held()` recompute 退化判据的依赖，kern_lock.rs :165-171，保留）。1b（合并刀）：把 PENDING/ROLL_PENDING 从 tlbshoot 全局数组迁入 `SmpCpu` per-CPU 块（`arch/aarch64/src/smp.rs` :119 起），与 preempt_count 同缓存行；增一位 want_resched（抢占诉求闩）。dispatch 尾改为：读一次 TPIDR → 单 load 该字 → 全零则三查全跳；任一位置位才走对应慢查与 preempt_checkpoint 的 DAIF 包络。置位点全集 = `broadcast`（闩，tlbshoot.rs :100-110）、`roll_quiesce_arm`（:177-187）、tick go 支（on_irq_tick 抢占分支）、SGI_RESCHEDULE 接收臂、RX woke 检查点、`fast_wake_locked`/`wake_matching` 入环点——这些点今天都已知道「我刚制造了切换/失效诉求」。

**影响估算**（推断）：44/45 号每号省 1 次 CNTVCT 读 + 3 次 MRS tpidr + 3 次 DAIF 系统寄存器操作 + 1 次间接调用 + 5 次全局写；TCG/icount 口径下系统寄存器 helper 是包络大头，此项约占整个固定包络的两至三成。真机口径下 CNTVCT 读与 DAIF 往返同为数十 cycle 级。

**可行性与风险**。1a 极高——纯诊断面裁剪，行为零变，注释自认「定谳后退役」，单文件单函数。1b 中——动 SmpCpu 布局（偏移断言面）、tlbshoot 迁闩、置位点梳理（漏置位 = 抢占延迟放大或 TLB 回执等待推迟，fail-loud 探针可沿用）；ktest/lockdep 面复核。验证：ktest + smoke + icount `--prove`（无语义变，基线随源刷新常规）。

## 机会二（分派机制）：TABLE 档「KL 全包」降类复审——0x61/0x87/0x40 落 NONE

**现状证据**。「NONE 实装号归零」纪律（`arch/aarch64/src/kern_lock.rs` :188-190；`svcface/src/lib.rs` :166-178 + host 钉板测试 `no_installed_none_class` :253-257）规定凡 copy_out/copy_in 链或任何共享面禁落 NONE——立法理由是 §A113 读者面：walk_leaf 无锁走查页表期间被跨核 unmap 撤页。但 S-4 4-2 起 copy 链已持 **per-AS PT 锁同窗**（`kernel/src/syscall.rs` :277-287/:311-319），且 vmo/vmar 十一号 ALL→TABLE 降档判例明确写着 PT 锁是「弃 KL 后的互斥后继」（install() :4106-4114 引 §A115 四问）。clock_read 的 TABLE 理由（:4099-4105）仍是 PT 锁时代之前的「无锁走查」表述——**判据已过时，落类未随源复审**。逐号核对三候选的共享面：0x61 `sys_clock_read`（:429-453）零 caller_table、零堆、零帧分配，唯一共享触点 copy_out_u64 已在 PT 锁窗内；0x87 `sys_signal_take`（:2752-2780）零 caller_table，pending 取走是 Relaxed 原子 swap（无锁需求），出参同 PT 锁窗；0x40 `sys_sched_util_snapshot`（:2691-2704）零 caller_table，EWMA 槽 Relaxed 原子读，copy_to_user 同窗。**反例（不可降）**：0x42/0x84 的 KLOG_SNAP_STAGE 单写者契约 = KL（:483/:507 注）；0x86 signal_send 碰 handle 表；0x88 power_off 走 UART 诊断 + PSCI。

**优化方案**。三号改注册 `SVC_CLASS_NONE`（install() :4105/:4159/:4170 三行 + svcface REGISTRY :101/:137/:147 同步）；「NONE 归零律」修订为「NONE 须过 PT 锁后继审计（§A115 四问）」——svcface 钉板测试与 kern_lock 注释同步；host 测试 `anchor_classes`（:305-311）改锚。

**影响估算**（推断）：三号每次调用省整对 kern_acquire/release（含机会一未做时的 CNTVCT 读 + 间接调用）+ SVC_WINDOW 两写；clock_read 是事件循环 deadline 换算的高频平凡号，GUI 负载下量级中高。若机会一先行，此项增量缩小但仍消一次 CAS + 全局行弹跳。

**可行性与风险**。高——每号一处常量 + 判例文 + 单源钉板，机械面小。风险低：SMP 下 sibling/parent 跨核 unmap 竞态由 PT 锁窗封死（判例在册），三号皆不触帧分配器/堆/表/跨 AS。验证：ktest（clock_read/signal_take 正负样本在册）+ smoke-smp + svcface host 测试。

## 机会三（快路径）：clock_read MONOTONIC vDSO 化——EL0VCTEN 已开，零 syscall

**现状证据**。主核 timer::init 已显式开 CNTKCTL_EL1.EL0VCTEN=1（`arch/aarch64/src/timer.rs` :83-99——注释自记这是为 bench1 用户态 `mrs cntvct_el0` 计时而开，Linux 同款），副核 bring-up 同开（`arch/aarch64/src/smp.rs` :653-658）。即 **EL0 直读 CNTVCT 的硬件门已全核打开**，而 `sys_clock_read` MONOTONIC 分支（`kernel/src/syscall.rs` :435-440）只是 CNTVCT/CNTFRQ 两次 MRS + 纯算术 + copy_out——一次完整 SVC 包络（帧 + 锁 + 尾税）换两条本可在用户态执行的指令。

**优化方案**。内核维护一个只读 vDSO 数据页：CNTFRQ 快照、MONOTONIC 基准、REALTIME epoch（seqlock 更新，rtc 注入时刷新）；`spawn_user` 装配期把该页 vmar_map 进每个进程固定只读 VA；用户态 libc 的 clock_read(MONOTONIC) 改为「读页 + mrs cntvct_el0 + 与内核同款的倒数乘-移换算（`ns_reciprocal_for` 单源可镜像）」，不陷 SVC。REALTIME 无注入时仍回落 syscall 或页内标记。ABI add-only：0x61 语义不变，只是高频路径不再到达。

**影响估算**（推断）：整号消灭——入口保存、锁类、尾税、handler 全免；Linux vDSO 经验中 clock_gettime 是用户态最高频 syscall 之一，事件循环/动画/bench 类负载受益直接。

**可行性与风险**。中——内核侧小（一个 .data 页 + map 钩子 + seqlock 写点）；用户态侧动 userbins/libc 链接与调用面（改动面跨 kernel/user 两仓）；freq 与 CNTFRQ 的单源纪律（页内快照须与 timer.rs FREQ 同源）；REALTIME epoch 跨核一致性（seqlock 读重试）。EL0 可读计数器无敏感面（计数器本非秘密，Linux/Android 同款暴露）。

## 机会四（入口保存）：SVC 专用快帧——只存 x0-x18 + x30

**现状证据**。EL0 SVC 槽（0x400）与 EL1 SPx 槽（0x200）都无条件走全量帧：`save_frame_el0` 16 存 + 5 MRS（`arch/aarch64/src/trap.rs` :224-250），`load_frame_el0` 19 读 + 3 MSR（:252-276），288B 帧布局 `#[repr(C)]` 常量断言钉死（:109-125）。而 SVC 语义只消费 x0-x6（handler 经 `tf.regs[1..6]` 读参）、只回写 x0（`arch/aarch64/src/syscall.rs` :86）。

**优化方案**。SVC 臂换专用宏 `save_frame_svc/load_frame_svc`：只存 x0-x18（AAPCS64 caller-clobbered 全集 + 平台寄存器 x18——LLVM 在 aarch64-none 目标可分配 x18，必须入集）+ x30（`bl` 第一跳即覆盖）+ esr/elr/spsr/sp_el0（**免 far**——EC=0x15 不写 FAR，读它是白付）；帧物理布局仍用 288B TrapFrame（x19-x29 槽不初始化），handler 签名、切换舞蹈、cbox/dump 面零改。fault 臂（BRK/D-abort/canary）保留全量帧不动。正确性论证：内核 Rust 代码全程守 AAPCS64 callee-saved 纪律（x19-x29）——handler 中途 block 切走时，舞蹈的 TaskContext 保的是内核态值，用户 x19-x29 由各层内核函数自身序幕/尾声保恢复，回到 asm 桩时寄存器仍是用户原值；这与 Linux/seL4 的 syscall 快入口同构。

**影响估算**（推断）：每号省 11 对存/取（22 次内存操作）+ 1 MRS，约占帧面四成、占整个固定包络一至两成；两向量槽代码尺寸同步缩。

**可行性与风险**。中低——asm 宏新双对、两向量槽换宏、EL1/EL0 双臂同改（ktest svc4/svc6 走 0x200 槽，`kernel/src/ktest.rs` :2473-2520）、fault/SVC 臂分流核验（`exception_sync`/`exception_sync_el0` 的非 SVC EC 面必须仍见全量帧——分流在向量槽层做，EC 判定前不知类别，故快帧只能用于「已确认是 SVC」的路径：要么向量槽全用快帧 + fault 时补存上半（二次保窗内无窗可插——不可行），要么 EC 先判后保（保存前先 mrs esr 判 EC=0x15——多一次 MRS 换 11 对存取，净赚仍明确）。此处是本刀的主要设计点）。icount 全家基线必变（指令流变），`--prove` 重立法。x18 归集已论证；x30 必存（BL 覆盖）。

## 机会五（分派机制）：间接调用/TPIDR 追链一次化 + SYSCALLS 表固化

**现状证据**。每号固定 4 次间接调用：SVC_WINDOW 槽 enter/exit 两调（`arch/aarch64/src/kern_lock.rs` :226-229/:243-246，`static mut Option<fn>` 无法去虚化）+ SVC_EXIT_CHECKPOINT 一调（`arch/aarch64/src/syscall.rs` :83-85）+ NAME_HOOK 一调（:153，机会一裁）。每次调用的钩子体第一件事是 current_task 追链：MRS tpidr + 判空 + volatile 载入（`arch/aarch64/src/smp.rs` :261-271）。同类追链每号还要再付约 3 次：handler 的 `caller_table`（`kernel/src/syscall.rs` :131-149 → `current_tag` → current_task）、结构锁位记账 `held_mark`（`sched/src/sched.rs` :499-505）、preempt_count 槽。合计 **MRS tpidr 约 5 次/号**，全部解析到同一 per-CPU 基址。另：`SYSCALLS` 派发表是 .bss 里的 `static mut` 函数指针表（`arch/aarch64/src/syscall.rs` :34-35），boot 期 install 后终生只读却保持可写页。

**优化方案**。5a：dispatch 入口读一次 TPIDR → 把 cur 指针写入本核 SmpCpu 的「本次 SVC 上下文」槽（或扩 SVC_WINDOW 钩子签名直传指针）；svc_window/caller_table/preempt_count/held_mark 改读同一缓存行；缓存槽失效点 = 切换舞蹈（切走清、切回置）。5b：install 完成后（enable_irq 前）把 SYSCALLS 表所在页 mmu 属性翻只读——函数指针表 W^X 固化，防运行期篡改的分派面（安全增益为主；推断：性能影响微，顺带消除该行误共享弹跳面）。

**影响估算**（推断）：每号省 2-3 次 MRS + 1-2 次间接调用；包络占比个位数到一成——是 Top-5 中单项最小但与机会一/二叠乘的清道夫项。

**可行性与风险**。5b 极高（一页属性翻转 + boot 序一行）。5a 中——跨 arch/sched/kernel 三 crate 热路径改签名/加槽，失效点漏清 = 旧 cur 悬垂（fail-loud 断言可执法）；与机会一 1b 的 per-CPU 合并行天然同刀共做。

## 落选记录（审了不进的）

- **派发表改 match 直呼/内联 handler**：256 槽表驱动 + 16B 依赖载入已是最优形态，match 瀑布反劣；且 class 位集随号走必须表查——不进。
- **nr 界检与默认槽合并**：越界号已零锁直达 -ENOTSUP（`arch/aarch64/src/syscall.rs` :59-65），无冗余——不进。
- **`-> !` 号（0x33/0x36）出口**：舞蹈收口设计自洽（switch_away_forever_with 按位放锁），无税可收——不进。
- **handler 逐个微操（如 clock_read 的除法换倒数乘-移）**：属 handler 语义面非分派面，且 timer.rs :77-82 已有倒数乘-移基建可镜像——归 03 路边界外（vDSO 刀顺带解决），单列不进。
- **EL1/EL0 双臂合一**：ktest/bench EL1 形态是测试面而非性能面——不进。

## 测量面建议（随任一刀附带）

icount 基线补一条 **svc_roundtrip**（EL1 `svc4` 打 0x61 或未实现号的 N 次往返，bench.rs 既有骨架 + ktest.rs :2473 svc4 直用）——把分派包络（帧存取+锁类+尾检查）纳入 L2 确定性门禁；现状该包络只有 B-1 墙钟通道的间接影子（`tools/bench.py` :268），任何包络刀（机会一/二/四/五）都无 deterministic 对拍面。此为 02 号姊妹件「send/recv 缺项」建议的同族补全。
