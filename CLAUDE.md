# CLAUDE.md

> 本文件由 Claude Code 每次会话自动加载，是本项目的工作上下文。
> 跨工具通用版（Cursor/Copilot/Codex 等）见 `AGENTS.md`，两者保持一致。

## 项目身份

**BerryOS** —— 自研的微内核 + 跨平台操作系统。
- 内核核心：**Rust**（`#![no_std]`，最小特权，只做调度 / IPC / 地址空间 / 能力）
- 用户态服务与工具：**Rust**
- 首批主线架构：**ARM64 (AArch64)**（v1 主战场 = 手表/穿戴 rich 档单线走到底，ADR #39）；**x86_64 提前启动 QEMU 最小集移植**（串口+定时器+异常向量+上下文切换+SVC 入口，范围盒子 + kill criteria，ADR #41）；RISC-V 留位。MCU 档（KB 级内存）v1 排除（ADR #40，覆盖面 = MMU 设备 MB 级起）
- 阶段沿革：**当前 Phase 4（IPC 与服务框架）进行中——4-0 睡眠/唤醒原语 + 4-1 阻塞 object_wait_one + 4-2 wait 面 ABI v0（位空间 + wait_one 扩 Channel + wait_many，4-2a/4-2b 双步收官）+ 4-3a 调度决策看 effective（pick 按 `prio_effective` 出选）+ 4-3b PI 捐赠（SLEEPQ 全图重推 + 冻结随行）已落地（2026-10）**；前情 **Phase 3（能力与系统调用）已收官——3-0 ~ 3-4c 全落地（2026-10）**。前情：**Phase 2b（地址空间 + 首个 EL0）已完成——2b-0 ~ 2b-4 全落地，出口关卡 G1.5 已过（2026-10）**。高半翻转：内核主体链到 `KERNEL_VA_BASE` 高半（镜像布局 VMA=KBASE+LMA → `.data/.bss` 零拷贝；`.text.boot` + 早期栈 + `.bss.boot` 留低半；`phys_to_virt` 加基址、va.rs 运行时注入；boot 串口 `KVA ok` 自校验 + smoke 程序头断言）。每进程页表 + ASID（16 位实测、0 保留、回卷全量失效）+ **低半身份表退役**（低半访问必 fault）。匿名 VMO/Vmar（`内核/src/vmo.rs`/`vmar.rs`，map 即填用户页表）。**2b-4 首个 EL0 + G1.5（收官）**：`TaskContext::new_user` 弹床 eret 降 EL0（SPSR=EL0t + DAIF 解蔽 → 用户态可被 timer 抢占）；异常走 lower-EL 向量 0x400/0x480（帧宏 `*_el0` 的 sp 槽=SP_EL0）；用户页 AP=0b01；三用户进程矩阵——`user_good` 打 'u'（syscall 往返 + 数据 canary），`user_rogue_ptr`/`user_rogue_svc` 各自 dump 后经 `user_fault_exit` 槽**只死自己**，watchdog 打 `G1.5 ok`（smoke 第 11 标记）。TCB 表示法字段（prio 双份/duration-hint/EPP/preemption-threshold）+ 时间片**周期数记账**（CNTVCT 增量扣减，DVFS 预埋）随 2b-4 落地。此前 Phase 2a（调度与任务，G1）/Phase 0-1（G0）已完成。**Phase 3（能力与系统调用）已收官——3-0 ~ 3-4c 全落地（2026-10）**：`内核/src/handle.rs` 内核对象壳 + handle 表（位域 = 槽号 16 位 + 代号 14 位、bit31 预留、槽 0 恒空；rights 12 位 + `RIGHTS_SAME` 哨兵；`KernelObj` 壳 v1 先 Vmo/Vmar + `ObjRef` 引用计数；五操作 insert/duplicate/close/replace/get——**权限在 handle 不在对象**、dup 只紧不松、close 后代号 +1 旧值不复活；门控顺序 查表→类型→权限）——纯内核层、外部行为零变化，ktest `HANDLE ok`（smoke 第 10 标记）。**3-1 真 syscall ABI 骨架已落地**：`内核/src/syscall.rs` 语义层（HAL `install` 注册口，同 IRQ 槽反转）——v1 27 号在册，实现 `handle_close/duplicate/replace`（syscall 全链路能力门控首次打通）+ `clock_read`(0x61)（MONOTONIC 纳秒）；**撤 2b-4「畸形号杀」政策**——未知/未实现号一律 -ENOTSUP 存活、EL0/EL1 同权（对齐 ABI §8.0「永不 panic」）；调试槽重排 NR_DEBUG_ECHO=0xFE / NR_DEBUG_PUTCHAR=0xFF（非 ABI，P4 退役）；引导表雏形 BOOTSTRAP_HT（3-2 挂进程）+ copy_out 种子（用户区经当前 TTBR0；内核高半仅 EL1 调用方，防 EL0 伪造内核写）；用户矩阵改板——good 改 clock_read 单调自检打 'u'，rogue_svc → **abuser**（真号+垃圾 handle → -EBADF 存活打 'S'），watchdog 断言杀数==1（只 rogue_ptr 死）。**3-2 VMO/Vmar syscall 全量门控已落地**：9 号全实现（0x11–0x19 create/commit/decommit/read/write/map/unmap/protect/allocate）——rights 矩阵（create 授 R|W|MAP|DUP、vmar map/unmap 需 MAP、protect/allocate 需 MANAGE、map 的 rights 须恰 READ|WRITE）+ `copy_from/to_user`（EL0 界检 `[0x1000,2^39)` + 逐页走查当前 TTBR0 验 AP 位；**无 TOCTOU**：SVC 入口硬件置 DAIF → syscall 主体不可抢占，表/对象免锁）+ **悬空 PTE 防线**（Vmo 每页 mapcnt；**销毁性 close（refs==1）遇映射未解除拒 -EBUSY、别名 close 放行**；decommit 拒映射中页；Drop 对映射中页只泄漏）+ no-panic 防线（vmo_create 上限 16MiB → 巨 size -ENOMEM，防 Vec abort）；**首能力实证**——good 的 root Vmar 进 BOOTSTRAP_HT 经 spawn arg 交用户，user_good 一次性验收链 `create→commit→write→map@0x5000→VA 读写→别名回读→close 拒-EBUSY→RO dup 写拒-EACCES→别名 close→unmap→decommit→close` 全绿才进稳态循环（链只跑一次：14 位代号经不起循环 create/close 耗 gen）；watchdog 升级 `last_killed()` **错杀断言**（kills==1 但死 good/abuser ≠ PASS——盲点曾真实放过一次错杀）。**3-3 process/thread 最小对象已落地**：`内核/src/process.rs`（Process = root Vmar 壳 + ttbr0/asid 快照；Thread = 未启动执行流描述，entry/sp 随 process_start 填）+ 三号 syscall（0x31 create / 0x32 start / 0x34 thread_create）——**对象间钉住**（Thread 钉 Process、Process 钉 root Vmar、start 再替任务钉 Process：`ObjRef::retain/release` 与 handle 计共用归零释放点，Drop 链不回表）；**`process_create` 双出参**（`*process,*vmar`——ABI §8.3 v1 修正，Zircon 同形：父须拿子地址空间根才能映代码）；v1 参数面：job 恒 0（**创建权未设防的已知豁口**，Job v2 收紧）、bootstrap 恒 0（per-process 表落地前无载体）；门控 process/thread 各 MANAGE + 归属校验（跨进程 start → -EACCES）；**引导表仍全局单表**（3-3 不拆表——3-4 channel 的 handle 迁移才是拆表硬前置）。验收 = 第 4 个 EL0 进程 `user_spawner` 经真 syscall 全链创建子进程 `user_child`（子打 'C'、spawner 打 's'，子代码字节内嵌在 spawner 自身代码页 `adr` 同页取址）；ktest F 段盖对象/错误面（ktest 在 sched::init 前跑、start 成功路径不在此验）。**3-4a per-process handle 表 + bootstrap 迁移已落地**：Process 自有 `ht: HandleTable`（出生为空，一切能力待父授入）、boot 手搭用户进程各挂 `Box::leak` 常驻表、`BOOTSTRAP_HT` 降格内核侧表（ktest EL1 面 + 未来 init 雏形）；**派发**：`Task::proc_tag`（sched 存不透明 usize、0=内核任务，语义归内核 crate——EL0 syscall `caller_table` 经 tag 查所属进程表，crate 边界干净）；**迁移原语 `take`/`insert_ref`**（take 摘槽代号 +1 原值作废、**refs 不变**=引用令牌迁移非销毁，映射中 VMO 照样可迁，EPTE 防线仍由最终 close 把守；消费者 = process_start bootstrap + 3-4b channel_send）；**bootstrap 迁移转正**（Zircon `zx_process_start` 同形）：非 0 时调用者表 take → 子表 insert → 子入口 x0=子表新 handle 值，迁移后发送方原值 -EBADF；验收 = spawner 建**礼物 VMO**（写 USER_MAGIC）作 bootstrap 送给子进程，`user_child` 凭**自己表里**的 handle `vmo_read` 出魔数比对打 'C'、spawner 迁移后 close 原值必 -EBADF 打 's'（链断自陷被杀 → watchdog 错杀断言 FAIL）；ktest G 段盖两表隔离/take 作废/insert_ref 可查/子进程出生空表。**3-4b channel 三号已落地**：`内核/src/channel.rs`（`ChannelEnd { peer: Option<NonNull<ObjRef>>, queue: Vec<Message> }`——create 建对、`pair()` 互指**互钉**各 retain 对端防 send 走 peer 悬空；消息 = inline 字节 ≤256 + 随行 handle 令牌 ≤4；队列深度 8）+ syscall 0x51/0x52/0x53——**v1 非阻塞**（空 recv -EAGAIN / 满 send -EBUSY，阻塞等待留 P4 object_wait）；**send 原子序**（参数上限 no-panic 前置 → RIGHT_SEND 门控 → 对端满检 → 随行值全量预检（`alive` 探活 + **数组内重复值拒 -EINVAL**——先 take 会作废后值）→ 载荷拷入 → 全量 take → 压对端队列，任一步失败调用者表零变化）；**recv 原子序**（空队先检 = 空时零 fault 面 → pop 消费 → 容量 in-out 不足 -EINVAL → 载荷拷出 → 令牌 insert_ref 入调用者表 + 新值回写，任一中途失败 `recv_rollback` take 回令牌 + 消息 `push_front` 压回队首保 FIFO）；**handle 随消息跨表迁移**（发送方 take 出表 close 原值必 -EBADF、接收方 insert_ref 落自己的表——与 3-4a bootstrap 同一对迁移原语）；**v1 泄漏政策（安全向）**：对端互钉成环两端全关不释放、消息中未收割令牌随队列泄漏、对端关闭检测不做（DEAD 检测 v3 收口）；ktest H 段八组（create 双端/空 -EAGAIN/往返/参数上限/深度满 -EBUSY/RO send -EACCES/handle 迁移闭环/send 原子性活值保表）。验收 = spawner/child 改板——spawner `channel_create` 建对 → `send(PING+礼物 VMO handle)`（能力经消息进子端点队列，close 礼物原值必 -EBADF）→ `process_start(bootstrap=end1)`（端点再迁子表，close 原值必 -EBADF）；child 凭自己表里的端点 recv 出 PING+礼物 → `vmo_read` 魔数比对（双重迁移闭环）→ 稳态收 PING 回 REPLY 打 'C'，spawner 发 PING 收 REPLY 验魔数打 's'（非阻塞双侧容忍 -EAGAIN/-EBUSY 无死锁）。**3-4c（Phase 3 收官）已落地**：①**AI 策略注入留桩**（`进程调度/src/policy.rs`——词汇表 = AI调度设计 §2 四类动作封顶：①放置 hint u8（v1 单核不消费）③包络内权重微调（`prio_effective = prio_base + clamp(weight, ±64)`，**包络是内核执法**）④EPP 二值；②频率 hint 走 PM 服务不经调度路径不入表；**没有第五类**；apply 幂等自基线重算非累积）挂 `RoundRobin::pick_next` 出选后——未绑/空表两分支即返零写入；②**event 三号**（`内核/src/event.rs` `Event{signals:u64}` 纯位掩码 + 0x54 create（options 恒 0、授 SIGNAL|DUPLICATE）/0x06 signal（`(s&!clear)|set`，门控 RIGHT_SIGNAL，v1 仅 Event）/0x04 **object_wait_one poll-only**（命中回写**全满足集** Zircon 同形；未命中 -EAGAIN；**`deadline≠0` 一律 -ENOTSUP**——阻塞排队留 P4，诚实缺口按号拒；等待不需权利位，12 位 rights 表无 WAIT 位）；ktest B 段未实现样本 0x04→0x05）；③**遥测环**（`内核/src/telemetry.rs` 静态预储 64×SwitchRec `#[repr(C)]`（vct/prev/next/kind/idle_next——ABI 形状 day-one 定型）+ `sched.rs` `set_switch_hook` 槽（三切换位 yield/preempt/exit 在 CURRENT=next 后 context_switch 前发射；**写入零分配**守 try 化批 IRQ 热路径纪律；任务 id = 名字 FNV-1a-32；idle 判据 kstack.is_none()；消费路径 telemetry_open 留 v3）；v1 读点 = watchdog 存活断言 `telemetry::total() ≥ switches()` + 环内 vct 旧→新单调）；④**空策略表等价测试**（ktest `POLICY ok` = smoke 第 13 标记：本地 Runqueue+RoundRobin 三任务 12 轮 pick——A 未绑基线 / B 绑空表**逐项全等**（接口在位+表空=零写入=与接口缺席逐位等价，「AI 不在线等价于纯启发式」的机器断言）/ C 对照表（p7 +32/Perf）**字段真变但选人序列不变**（apply 反向控制防死代码假等价 + RR 不看优先级 = 当时护栏，**4-3a 起撤**——pick 看 effective，C 断言升级为 eff=32 者独占出选，见 4-3a 段）/ 收尾 unbind；进程调度 crate 无 host 测试面，QEMU ktest 是唯一家）；syscall 实现数 19→**22**（余 -ENOTSUP：0x05/0x33/0x35–0x37）。**4-0 睡眠/唤醒原语已落地**（Phase 4 拆步首步，表见 `文档/IPC设计.md` 阶段化目标）：`TaskState::Blocked` + `WakeReason`（Pending/Signaled/Timeout）+ Task 六睡眠字段（`sleep_prev/next` 侵入式链接、`wait_obj/wait_signals` 匹配键、`wait_deadline` CNTVCT 周期、`wakeup`）+ 全局 `SLEEPQ` **单队列**（按 deadline 升序、相等 FIFO——超时扫描头未到期即早停；**单队列匹配唤醒**而非「对象 waitq + 超时队列双挂」：Task 只有一对睡眠链接字段，双挂一致性易错，唤醒方按 obj+掩码遍历匹配，v1 等待者个位数 O(n) 无忧）；`block_current(obj,signals,deadline)`（**屏蔽窗内**填字段→入 SLEEPQ→**出 runqueue 环**（Blocked 必须摘环——rotate 会选中它；`RR.dequeue` 置 Exited 语义不符，直接 `Runqueue::unlink`+手置 Blocked）→pick next or idle→切走；单核屏蔽窗原子 = 无丢唤醒/虚假唤醒，SMP(G2) 重做；**续行时 IRQ 仍屏蔽**——SVC 语境勿解蔽（eret 自然恢复 SPSR），内核任务语境自行 `enable_irq`）+ `wake_matching(obj,signals)`（遍历摘「obj 匹配且掩码相交」者置 Signaled 入环；obj=0 非法返 0——纯 sleep 只能超时自醒；不立即抢占等 tick/yield）+ `on_irq_tick` 开头超时扫描（tick 粗粒度：到期后最迟一个 slice 摘；tickless 精确 arm 留后）+ **idle_loop SLEEPQ 感知**（runqueue 空但 SLEEPQ 非空 → 不 mask CNTV、arm tick 空转扫超时——睡眠者不会被永睡饿死）；`thread_sleep`(0x37) **首个阻塞 syscall**（deadline = 绝对 MONOTONIC ns，与 clock_read 对称先商后余换算防溢出；已过点立即返 0；SVC 天然屏蔽窗、免锁根基不破）；`SwitchKind::Block`（遥测 kind=3）。**唤醒遥测环配套落地**（AI调度设计 §9 留项「唤醒事件」收口）：Task `wait_since` 入睡戳（与 deadline 同屏蔽窗填）+ `sched.rs` 唤醒钩子槽（`set_wake_hook`/`note_wake`——两唤醒位 `wake_matching`(Signaled)/`expire_timeouts`(Timeout) 在摘队置因入 runqueue 后发射，任务字段已定型、`wait_obj/wait_since` 唤醒不清可读；同款零分配零切换纪律）+ `telemetry.rs` 第二环 `WAKE_RING` 64×`WakeRec` `#[repr(C)]` 32B 全定宽（vct/since_vct/wait_obj/task_id/reason——block→wake 延迟即 §4 第一级「唤醒模式」数据，ABI 形状 day-one 定型）；watchdog 加断言（`BLOCK_BOOT_DONE` 门槛——sleeper 双路径全过才验，不赌 boot 时序：wake_total≥2/延迟非负/vct 单调/醒因合法，环未淘汰期 total≤64 才断双醒因覆盖——淘汰后掩码只反映环内现状）；消费路径 telemetry_open 仍留 v3。验收 = boot spawn sleeper/waker 对打（ktest 在 sched::init 前跑不能睡）：sleeper 路径 1 纯睡 1 slice 自醒断言 Timeout+时长、路径 2 挂静态 SLEEP_OBJ 无限等被 waker 轮询 `wake_matching` 摘醒断言 Signaled（单核屏蔽窗 = 轮询无丢唤醒的活证），双绿打 `BLOCK ok`（**smoke 第 14 标记**）；run-el2 同过。syscall 22→**23** 号。**4-1 阻塞 object_wait_one 已落地**：`sys_object_wait_one` 改板——**poll-first**（信号已到即返不睡，无丢信号）→ 未命中且 `deadline≠0` 挂 `block_current` 睡到绝对时刻；**醒来重验条件**（醒因 ≠ 结果：Timeout 时位可能刚被置、Signaled 时位可能又被清——清位竞态下未命中且未到点则**再睡**，统一循环兜底）→ 到点未命中 **-ETIMEDOUT**（`ERR_TIMEDOUT=-110` 新增，ABI §8.2 在册）；`deadline==0` poll 语义不变（-EAGAIN）；**钉住纪律的跨睡眠版**：进循环前 `ObjRef::retain` 一次覆盖全部睡眠轮次（睡眠期间全表 handle 被关光也不销毁——waker 只持另一表 handle 的场景）、release 收口在唯一出口（归零 drop 在屏蔽窗内、Drop 链不回表，安全；**release 后不得再触 obj**——若逐轮 retain/release，归零后再循环即 UAF）。`sys_object_signal` 置位后 `wake_matching(ObjRef 地址, set)`——唤醒等同一对象、请求位与**新置位集**相交的睡眠者（纯清位无新增位 → 无人醒；重置已置位 → 假唤醒无害，被唤醒者重验兜底）；**匹配键 = ObjRef 地址**（同一 Event 的 handle 跨表 insert_ref 共享同一 ObjRef → 双表 signal/wait 天然互通）。ktest I1 改板：deadline=1（远古时刻）→ -110 **不睡**（ktest 在 sched::init 前不能睡，「已过点立即返」路径）。验收 = 第 5 个 EL0 进程 `user_waiter`（打 'W'）三路 + 内核任务 `waker2`：①超时（清掩码 → wait(bit3, t0+100ms) → -ETIMEDOUT + observed==0 + t1≥deadline 真睡到点）②poll 命中（signal(!0,0x20) 置恰 bit5 → wait 请求超集 0x220 → 即刻返 0 + observed==0x20 **全满足集非请求子集**）③长睡（10s deadline）真睡 → waker2 经**真 SVC 链**（clock_read → thread_sleep 3 slice → `object_signal(内核侧表 handle, 0, bit3)` → 再睡 1 slice）唤醒 → observed==0x8；同一 Event **双表挂同一 ObjRef**（boot：bootstrap 表 insert + get_raw 取指针 + waiter 表 insert_ref）——signal 侧持内核表 handle、wait 侧持自己表 handle，能力跨表互通活证；waiter 断链被杀 → watchdog 错杀断言拦下；waker2 打 `WAIT ok`（**smoke 第 15 标记**）。**4-2a wait 面 ABI v0（前半）已落地**：信号位空间 = 低 32 位内核派生 `SIGNAL_READABLE`(1<<0)/`SIGNAL_WRITABLE`(1<<1) + 高 32 位用户 `SIGNAL_USER_MASK`(0xFFFF_FFFF_0000_0000)（Zircon 界）；`object_signal` clear/set 掩码**只收用户半**——越界 -EINVAL（检在 get_raw **之后**：坏 handle/类型仍 -EBADF 优先，ktest I5 面不乱）；Event 位面全量迁高半（ktest I 段 U1/U2/U3=1<<33/34/35、user_waiter 三路、waker2 signal 1<<35——**清零掩码用用户半全集**，旧 `!0` 清零 4-2 起非法）。`wait_one` 扩 **Event|Channel**：`signal_state` 统一查询点（Event=用户位掩码；Channel=READABLE(本端队列非空)|WRITABLE(对端队列未满)——内核派生，用户 signal 不出此位；wait 低 32 位请求 Channel = 永不命中，同 Zircon 不设门）；**唤醒点**——send 成功 `wake_matching(对端 ObjRef, READABLE)`、recv 成功 `wake_matching(对端 ObjRef, WRITABLE)`（A 的可写性由 B 队列决定；仅成功路径——回滚路径状态复原，唤醒即虚假）；channel.rs 头注「阻塞等待留 P4」注记收口。ktest I6 低半拒面（set/clear 各一 + Event 低半请求 -EAGAIN）+ I7 Channel poll 面全套（空 -EAGAIN/出生 WRITABLE 命中/send 后 READABLE 命中且全满足集含对端空位 WRITABLE/灌满 -EAGAIN/recv 腾位 WRITABLE 命中/混合位全满足集=两派生位之并）。验收 = 第 6/7 个 EL0 进程 **pinger/ponger** 对打：boot 手搭 channel 对（两表各 insert 一端 + `pair` 互钉——`channel_create` 建对语义的 boot 版；pinger 持 endA RIGHT_SEND、ponger 持 endB RIGHT_RECV）；pinger **EBUSY 驱动**验收链——send 灌到 -EBUSY（满即证明，不赌精确计数——timer 抢占可能让对端中途腾位）→ `wait_one(WRITABLE, now+10s)` **必须命中**（ponger recv 腾位唤醒；超时=链断自陷）→ 补发一条须成功 → 首圈打 `CHAIN ok`（**smoke 第 16 标记**，banner 9 字节经同页 `adr` 取址 + `.balign 4` 尾对齐——模板长度须 4 字节倍数）→ 稳态每圈节流打 'p'；ponger 循环 recv / -EAGAIN → `wait_one(READABLE)` 阻塞 → 醒来重验收得 PING 验载荷低 32 位 → 节流打 'P'。稳态 = 生产者常态阻塞 WRITABLE——「send 满 → 对端 recv 腾位唤醒」两进程阻塞链活证；watchdog 之后自陷的晚失败由 CHAIN 标记缺失兜底（smoke 即关卡）。**4-2b `wait_many`(0x05)（4-2 收官）已落地**：n∈[1,16] 任一命中出 `*idx`（入参序最前命中项）+ `*observed`（该对象全满足集），poll（deadline==0）-EAGAIN / 到点 -ETIMEDOUT 与 wait_one 同口径；**WAIT_ANY 哨兵**（`berry_sched::WAIT_ANY`=usize::MAX——Task 只有单一 wait_obj 匹配键，多对象等待者挂哨兵，`wake_matching` 谓词加哨兵分支对「任何对象、任何新置位」命中，醒来重验循环定夺——虚假唤醒无害，v1 等待者个位数全队列误醒开销可忽略）；两数组（handles/signals）**先整段 copy_from_user 拷入内核栈**再解析（字节缓冲 + 逐项小端拼，同 channel_send 随行值模式；无 TOCTOU）；数组内重复 handle 合法（各项独立 retain）；坏 handle 解析即拒**首错透传**（无 retain 即无回滚面）；**钉住纪律多数组版**——全部对象各 retain 一次覆盖全部睡眠轮次、唯一出口逐个 release。ktest I8 盖错误面+poll 面（n=0/n=17 → -EINVAL；坏 handle → -EBADF；poll 未命中 -EAGAIN 且 `*idx` 留预清哨兵 u64::MAX；远古 deadline=1 → -ETIMEDOUT 不睡；命中 idx 两向活证——首项/次项交换各验一次）。验收 = ponger 稳态等待改 **wait_many n=2**：[endB:READABLE, boot 直插哑 Event:1<<32 永不 signal]——命中须 `*idx==0`（哑 Event 位永不置、idx==1 即断链自陷被杀）；首次命中打 `MANY ok`（**smoke 第 17 标记**，banner 8 字节恰好 4 对齐）；哑 Event handle=0x0001_0002 **boot 断言定值**（ponger 模板硬编码依赖——表内第 2 插=槽 2/代号 1，表序有变断言即拦）；ktest B 段「未实现真号」样本 0x05→0x33。syscall 23→**24** 号。**4-3a 调度决策看 effective 已落地**（`调度器设计.md` §3.6「调度决策一律看 effective」的 v1 兑现，4-3 三小步首步——PI 捐赠与 AI 权重微调的共同消费点）：`RoundRobin::pick_next` 改按 `prio_effective` 出选——**两圈扫描**（一圈记最高 effective、二圈从 head 起取第一个 == 最高者——同高 FIFO，head 侧即等待最久者）+ 选中后游标前移其后继（旧 `rotate()` 退役，语义拆进 `Runqueue::head()` + `advance_past()` 两个遍历原语）——**同 effective 层内轮转、跨层严格优先**；全员同 effective（v1 常态 prio 全 0）时与纯 rotate 逐轮等价 = 既有矩阵零扰动（run 实证：稳态 A/B/u/W/S/s/C/p/P 轮转 + switches 金丝雀不变）。**POLICY ok 语义升级**：C 用例断言从「注入不改选人序列」（3-4c 护栏，RR 不看优先级）改为「首轮全 eff=0 出选同基线 head、此后 eff=32 者独占出选」（严格优先的机器断言）；A/B 空态等价不变。饥饿面 v1 接受（PI boost 是临时态、AI policy 未启用、稳态矩阵全 prio 0 零影响；公平回退留 v2 fair class）；`wake_matching` 注释同步（仍不立即抢占，但下一轮 pick 看 eff 自然优先）。**4-3b PI 捐赠已落地**（`内核/src/pi.rs`，`IPC设计.md` §2.1 QNX 发送者语义的 v1 形态）：微内核服务化后软实时命门在 IPC（高优 client 阻塞 channel、低优 server 拿不到 CPU，Mars Pathfinder 案例）——BerryOS 有界队列（深 8）把 QNX「双方同睡一个 channel」解耦（接收方持 peer handle 可睡任何对象），故捐赠边靠 **handle 表反查**：donor = SLEEPQ 上睡单一 channel 等内核派生位（READABLE/WRITABLE）者；受捐 = 同在队、其 handle 表**持 peer 端点且带解锁 right** 的任务（等 WRITABLE 需 RECV、等 READABLE 需 SEND，多持有者全捐）——`holder_has`（handle.rs）+ `donor_edge`/`holder_table`（pi.rs；内核任务 proc_tag=0 → 内核侧共享表 = 所有内核睡眠者都算持有的 v1 粒度，EL0 每进程表才反查到人）；Event donor / WAIT_ANY 哨兵 / 纯 sleep / 只等用户位均无边（诚实缺口：Event 捐赠 v2 再议）。**全图重推零增量簿记**：SLEEPQ 变更（block 睡下后 / wake 摘队后，`set_sleepq_hook` 注入）+ handle close 后从零推导——清零在队者 → 不动点迭代 ≤`PI_MAX_HOPS`(8) 轮（每轮传播一跳；**d_eff 实时读 `pi_inherited`**——只读尾趟才刷的 `prio_effective` 会把链式传播截死一跳，ktest J2 的机器断言面）→ 尾趟刷在队者 effective；持有环 max 单调有界不发散、轮帽 = 阻塞链深 8 执法点（到帽按已达 boost 处理不 panic）。**冻结语义**（「server 处理消息期间持有」的 v1 形态）：重推只清/算在队者，受捐方唤醒离队时冻结 `pi_inherited` 随行（boost 带进 runqueue——若无冻结，wake 重推清零会让 boost 在 pick 前丢失），下次入睡被重推清零回落（=「reply 即回退」的无 reply 等价物）；长跑不睡者 boost 挂着（QNX 语义本就如此，v1 接受）。合成公式 `prio_effective = max(prio_base + 策略权重, pi_inherited)`（policy::apply 与 refresh_effective 同式不互踩）。**零分配纪律**：recompute 挂 IRQ 屏蔽热路径，全程只读指针写字段（3-4c try 化批堵的 OOM 面不得重开）。Task 加 `pi_inherited` 字段 + `spawn_with(entry,arg,name,prio)` + sched 导出 `for_each_sleeper/refresh_effective/sleepq_insert/sleepq_unlink/set_sleepq_hook`。ktest J 段（`PI-K ok`，test_pi）：手搭睡眠者（`sleepq_insert` + leak 私有表指针当 proc_tag——EL0 每进程表同语义）直调 recompute——J0 单跳判别 / J1 rights 两向门控 / J2 链式两跳（实时 pi 传播）/ J3 持有环收敛（双 5）/ J4 11 节点链**倒序入队**（deadline 相等 FIFO → donor 访问序 L10..L0，每轮恰传播一跳：L8 达 40、L9/L10 超帽落 0）/ J5 哨兵与 Event donor 无边 / J6 冻结随行 + 重入队回落 / J7 共享表粒度（含自捐无害面：pi ≤ 自身 d_eff 不膨胀）。验收 = boot 三内核任务全走真 SVC：pi_client(32) send×8 + 第 9 条 -EBUSY → wait WRITABLE（100s 帽）睡；pi_server(0) 睡 Event E（1<<35，100s 帽）——block 钩子重推成边 server 受捐 32；pi_probe 睡 5 slice 醒来 `for_each_sleeper` 断言（server pi==32 且 eff==32、client eff==32）→ signal E → 打 `PI ok`（**smoke 第 18 标记**）；server 醒（冻结 32 随行）recv×8（第一条腾位即唤醒 client）、client 醒补发成功自证，双双 exit。**4-0 sleeper 信号位收窄**（donor_edge 不变量的连带修）：请求位含内核派生位 ⟹ wait_obj 必须是真 channel ObjRef——4-0 合成对象 SLEEP_OBJ（u8 静态，不透明 token）曾以 `!0` 睡（含派生位）触发 pi.rs 对齐 panic（run 实跑抓到），改等用户位 1<<33（内核永不置位、waker 置位集同步收窄；bit33 与既有全部等待位零碰撞）。下一步 4-3c intent 五件套（§2.1/§3.6）→ 4-3 收官。
- **身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 干涉放置/频率/优先级，见 `文档/AI调度设计.md`）+ PC 高性能。

## 设计总原则（三条，钦定）

1. **人无我有，人有我强**：博采众长的落地口诀——别人验证过的优点学过来，别人踩过的坑绕开。证据库 = `调研报告/`（各家好在哪/不好在哪/怎么做的）；借鉴任何参考前先过 `文档/架构综合纲领.md` 排除 8 问，防杂糅。软件行业积累到今天是做 OS 最好的时代：成熟参考实现全部开源可得，站在巨人肩上是常态而非例外。
2. **AI 友好化**：把 AI 当系统的一等公民——不止 AI 原生调度（`文档/AI调度设计.md`），一切接口对 AI 可读、可推断：ABI 单一事实源生成（`文档/系统调用ABI设计.md` §8）、结构化 tracing（`文档/可观测性设计.md`）、语义化错误码、机器可读文档。
3. **AI 测试友好化**：为"以后都是 AI 测软件"的时代预埋基建——测试基建是产品的一部分，不是附属品：确定性可复现（同输入同输出）、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（`文档/测试策略.md` G0–G5）；目标是 AI agent 能独立跑测试、读结果、报 bug。

## 架构决策（已定，详见 `文档/架构决策.md`）

四轮设计问答锁定。改任何相关代码前先对齐：

| 维度 | 决策 |
|------|------|
| 哲学 | Zircon 务实派（内核有对象/分配器） |
| 权限 | 纯能力（Zircon 内核态 handle） |
| 内存 | VMO + 用户态 pager + 每进程独立 AS |
| IPC | 全支持：同步 channel + 异步通知 + 共享内存；路径式命名空间 |
| 并发 | 细粒度锁 + 全可抢占（实施分阶段） |
| syscall | 中等偏上 ~30–45（v1 仅 ~26，用 `object_*` 合并）；FS/网络走 IPC |
| 驱动 | 用户态为主，启动关键（GIC/timer/UART）暂留内核 |
| 启动 | QEMU 直加载（`-kernel`） |
| 软件 | 自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**） |
| 调度/电源 | 双线并行（见 `调度器设计.md`） |
| 语言/架构 | **全 Rust**（内核 `#![no_std]` + 用户态）；syscall 边界用 C-ABI 约定；ARM64 首发 |
| **端定位** | v1 = 手表 rich 档（MB 级内存）ARM64 单线；MCU 档排除；v2 攻工业 RT（对齐 6.12 PREEMPT_RT 量级）；x86 = 兼容验收 + QEMU 最小集提前（XNU 同源分端；ADR #39–#41） |
| 实时/FS | 软实时；MVP 用 FAT |
| **分阶段基线** | v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（见各文档「阶段化目标」+ `测试策略.md` 关卡 G0–G5） |
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`）。**2026-10-16 修订**：干涉面限带内、AI 降级为机制预留+实验轨道+kill criteria |
| **调度 intent** | intent=第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42） |

## 微内核设计原则（写代码前必须遵守）

1. **内核极简**：内核只保留不可委托的能力 —— 调度器、IPC、地址空间管理、能力（capability）派发。文件系统、网络、设备驱动**统统放用户态**，作为服务进程。
2. **IPC 是中枢**：服务之间、服务与内核之间通过显式消息传递通信，禁止内核内直接共享可变状态。
3. **能力即权限**：资源访问通过 capability 句柄传递，而非全局 UID/权限位检查。
4. **硬件抽象隔离**：所有架构相关代码收敛在 `架构相关/<arch>/` 下，核心逻辑写成与架构无关的 Rust，移植新架构只动 HAL 层。
5. **失败隔离**：单个用户态服务崩溃不应拖垮系统，内核需提供重启/监督机制。

## 调度器与电源（设计已定，详见 `文档/调度器设计.md`）

改动任何调度 / 电源相关代码前，先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；调度策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手表(单核小算力)→手机(大小核)→PC(全对称)；拓扑由 `架构相关/<arch>/` 填表，核心逻辑架构无关。一条放置路径三端通用。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断，续航第一杠杆，不是后期补丁。
- **每核 runqueue + 负载均衡**：多核吞吐主力；单/少核平台近零开销。
- **电源管理是用户态 Rust 服务**：DVFS governor / 热管理 / 休眠策略推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 消息拷贝，否则吃延迟）；PM 服务可崩，内核降级保活、监督重启。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电（tickless/idle/PM）与多核性能同为 v1 一等公民，路线见设计文档 M1a/M1b。
- **intent 两段式（ADR #42，2026-10-16）**：**公平是机制默认，intent 是策略输入，叠加永不替代**。v1/v2 intent = u8 查表键（IntentProfile→latency-nice/EPP/uclamp/timer slack；TCB `intent_declared/effective` P3 预埋、`berry_intent` 编码冻结 0x00 UNSPECIFIED(XNU 式不设限)–0x05 MAINTENANCE(冻结不启用)）；M4 起 class_fair ×4 带实例 + 预算包络 + deadline-server 兜底，**凭 v1 遥测入场券**（不满足则单实例即终态）；三层拓扑 class 链 > band 序 > 带内 EEVDF；`SchedClass` trait 签名零改动；band/intent **不入 AI 动作空间**。

## 仓库布局

```
berryos/
├── 内核/              Rust no_std：调度 / IPC / 地址空间 / 能力
├── 内存管理/          Rust no_std：物理页帧 / 虚拟内存 / 页表
├── 进程调度/          Rust no_std：调度器 / 线程 / 上下文切换
├── 进程间通信/        Rust no_std：消息传递 / 共享内存 / 信号量
├── 架构相关/          Rust：HAL + 内联汇编（规划 架构相关/aarch64/）
├── 引导加载器/        Rust：boot / 启动协议
├── 驱动/              Rust：用户态驱动服务
├── 文件系统/          Rust：VFS / 具体 FS（用户态服务）
├── 网络/              Rust：协议栈（用户态服务）
├── 用户态/            Rust：libc / shell / 系统服务
├── 工具链/            构建脚本（cargo + just）/ Rust target 配置
├── 文档/              设计文档 / 笔记
├── 参考源码/          第三方参考实现，独立存放（见其内 README.md）
├── CLAUDE.md
└── AGENTS.md
```

> 全 Rust：内核类目录是 `#![no_std]` freestanding Rust（不链接 std/alloc，自带分配器），服务类目录是普通 Rust。**syscall 边界用 C-ABI 约定**（Rust ABI 不稳定），两端都是 Rust——数据用 `#[repr(C)]`。`架构相关/` 内按平台分子目录（如 `架构相关/aarch64/`）。

## 设计文档

- `文档/路线图.md` —— 宏观开发路线（Phase 0–8，每阶段一个 QEMU 里程碑）。
- `文档/开发体验.md` —— Mac 上的运行/安装四级阶梯（QEMU 直加载 → initrd → fat:rw → 自举镜像）+ 交叉编译日常循环 + hvf/TCG 双模式；P4 G-eco / P5 shell / P6 镜像的体验路线。
- `文档/shell设计.md` —— P5 交互 shell（bsh）：不搬 bash/zsh（GPL/fork 依赖/太重），命令三梯队（uutils MIT 基础 + 自研开发 + berryctl/powertop 独有）、channel 即 pipe、`--json` AI 友好输出。
- `文档/调度器设计.md` —— 调度器与电源管理详细设计（机制/策略/capacity/tickless/PM 服务）。
- `文档/架构决策.md` —— 全部决策的单一事实来源（ADR）。
- `文档/能力模型设计.md` —— 纯能力 + Zircon handle。
- `文档/IPC设计.md` —— channel/通知/共享内存 + 路径式命名空间。
- `文档/内存管理设计.md` —— VMO + 用户态 pager + 每进程 AS。
- `文档/系统调用ABI设计.md` —— 中等偏上 ~30–45 syscall（v1~26，具体编号表）+ POSIX 子集边界（v1 无 fork）。
- `文档/测试策略.md` —— 关卡式验证（G0–G5）：单核正确 → 抢占 → SMP → 全抢占 → demand paging。高并发可抢占内核的命门。
- `文档/AI调度设计.md` —— AI 原生调度：策略注入/遥测 ABI、安全护栏、平台分层、(B)启发式→(A)学习两路线。
- `文档/安全与硬件隔离设计.md` —— IOMMU/SMMU + PAC/BTI/MTE + 度量启动（能力安全的硬件后盾）。
- `文档/可观测性设计.md` —— day-one 内置 tracing（静态点 + per-CPU 无锁环 + trace 服务）。
- `文档/文件系统设计.md` —— FAT 仅 MVP；目标现代 FS（CoW/日志/校验）。
- `文档/架构综合纲领.md` —— **综合视角（博采众长不杂糅）**：六个参考（Redox/Theseus/rCore/hubris/seL4/Zircon）的取/留、矛盾矩阵 10 轴的唯一 PICK、防杂糅的排除 8 问；开篇 §0 系统四层速览图（层/目录/时点 + 两条微内核铁律）。借鉴任何参考前先读。

## 语言策略与 syscall 边界（全 Rust）

| 层 | 形态 | 备注 |
|----|------|------|
| 微内核核心、HAL、引导早期 | **Rust `#![no_std]`** | 不链接 std/alloc；自带分配器；`panic=halt`；汇编用 `core::arch::asm!` / `global_asm!`（EL 检测、MMU、异常向量、上下文切换） |
| 用户态系统服务、库、工具链、用户程序 | **Rust** | 服务 `#![no_std]` 起步，高层工具可上 `std` |
| **syscall 边界** | **C-ABI 约定**（两端皆 Rust） | Rust ABI 不稳定，故 syscall 入口/数据结构用 `extern "C"` + `#[repr(C)]`；内核与用户态都是 Rust，无跨语言 FFI 风险 |

约定：
- **`unsafe` 策略**：`unsafe` 收敛到小块——页表、上下文切换/寄存器保存、MMIO、分配器、硬件原子操作；外层包安全 API。内核 = "安全 Rust 外壳 + 小 unsafe 核"。
- Rust 侧绝不持有跨 syscall 边界的引用生命周期假设；跨越边界的数据按拥有的/复制的传递。
- 公共 ABI（`berry_*` + 结构）用 `#[repr(C)]`，内核与 `berry-sys` 镜像同步（见 `系统调用ABI设计.md`）。

## ARM64 目标约定

- 默认目标：`aarch64-unknown-none`（内核 + 用户态 no_std）；如需特殊段布局用自定义 `target.json` + linker script。
- 内核运行在 EL1，用户态 EL0；**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`-machine virt,virtualization=on` 则 EL2），EL 检测/降级在 `架构相关/aarch64/` 集中处理（`mrs CurrentEL` 判别 → 按需 eret 到 EL1）。异常级别切换亦在此。
- 调试与验证主用 **QEMU `qemu-system-aarch64`**（virt 机型）。

## 构建与测试

Phase 2b 已收官（G1.5 过关）、Phase 3-0（handle 表）/3-1（syscall ABI 骨架）/3-2（VMO/Vmar syscall 全量门控）/3-3（process/thread 最小对象）/3-4a（per-process handle 表 + bootstrap 迁移）/3-4b（channel 三号 + handle 随消息迁移）/3-4c（event 三号 + 遥测环 + 空策略表等价测试——Phase 3 收官）/4-0（睡眠/唤醒原语 + thread_sleep 首个阻塞 syscall）/4-1（阻塞 object_wait_one + signal 遍历唤醒）/**4-2（wait 面 ABI v0：信号位空间 + wait_one 扩 Channel + wait_many，4-2a/4-2b 双步）/4-3a（调度决策看 effective——pick 按 `prio_effective` 出选，同 eff 层内轮转跨层严格优先）已落地**：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（TTBR1 高半别名自校验，2b-2 起只走高半）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok`（3-2 起含 VMO/Vmar 九号全链 E 段；3-3 起加 F 段：process_create 双出参/参数面/thread_create 门控/process_start 参数与归属面/钉住释放链；3-4a 起加 G 段：两表隔离/take 迁移原值作废/insert_ref 落新表可查/子进程出生空表；3-4b 起加 H 段：channel create 双端/空 recv -EAGAIN/send-recv 往返/参数上限/深度满 -EBUSY/RO send -EACCES/handle 随消息迁移/send 原子性；3-4c 起加 I 段：event create options 拒/poll 未命中 -EAGAIN/deadline=1 远古时刻 -ETIMEDOUT 不睡（4-1 改板）/signal 置清位/全满足集回写/dup 无 SIGNAL -EACCES 但 wait 照常/类型野值 -EBADF；4-2 起 I6/I7/I8 扩 Channel 与 wait_many 面；4-3b 起加 J 段：PI 图推导八用例（单跳/rights 门控/链式两跳/持有环收敛/11 链轮帽截断/哨兵与 Event 跳过/冻结回落/共享表粒度））→ `PING TASK ok`（switch asm 乒乓）→ 低半读期望故障恢复（`-> expected D-abort`）→ `ASPACE ok`（两张用户页表同 VA 隔离 + asid=16b）→ `VMO ok`（VMO/Vmar 六操作 + unmap 后读 fault、RO 写 fault 两次期望故障）→ `HANDLE ok`（3-0 handle 表五操作 + 槽位重用代号 +1）→ `POLICY ok`（3-4c 空策略表等价 + 4-3a 真消费：A 未绑基线/B 绑空表逐项全等/C 对照表 eff=32 者独占出选——严格优先）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 与 EL0 用户进程 `user_good`（打 'u'）/`user_abuser`（打 'S'）/`user_spawner`（打 's'）及其子进程 `user_child`（打 'C'）在 CNTV timer slice 下交替（A/B 每次打印先读私有页断言隔离）**，周期打 `switches=N` 金丝雀；user_good 入口先跑 3-2 一次性验收链（经真 syscall create→commit→write→map→VA 读写→别名回读→close 拒/RO 拒/unmap→decommit→close）全绿才进稳态循环；**3-4b 验收：spawner 与 child 经 channel 交换消息**——spawner 一次性链 process_create（双 handle）→vmo 三步→vmar_map 进子空间→建礼物 VMO（写 USER_MAGIC）→channel_create 建对→send(PING+礼物 handle)（迁移自证：close 原值必 -EBADF）→thread_create→process_start(bootstrap=end1)（再迁移，close 原值必 -EBADF）；child 凭自己表里的端点收 PING+礼物 → vmo_read 魔数比对 → 稳态收 PING 回 REPLY 打 'C'，spawner 发 PING 收 REPLY 验魔数打 's'（非阻塞双侧容忍，无死锁）；恶意用户进程 rogue_ptr（野指针）dump + 被杀（**只死自己**），abuser（坏 handle 滥用）-EBADF 存活——watchdog 断言杀数==1 **且死者是 rogue_ptr**（`last_killed()` 错杀断言）+ **3-4c 遥测环存活断言**（`telemetry::total() ≥ switches()` + 环内 vct 单调）+ **唤醒环断言**（`BLOCK_BOOT_DONE` 门槛后：wake_total≥2 / 延迟非负 / vct 单调 / 环未淘汰期双醒因 Signaled|Timeout 覆盖）打 `G1.5 ok`；**4-0 验收：sleeper/waker 对打**——sleeper 超时自醒（'T'，断言 Timeout+时长）+ 条件唤醒（'K'，挂 SLEEP_OBJ 被 waker 轮询 `wake_matching` 摘醒断言 Signaled）双绿打 `BLOCK ok`，此后两者 exit 退出、稳态矩阵无扰；**4-1 验收：user_waiter（打 'W'）三路 + waker2 对打**——waiter 超时（-ETIMEDOUT + observed==0 + 真睡到点）/poll 命中（恰置 bit5 → 请求超集 → observed==0x20 全满足集）/真睡（10s deadline）被 waker2 真 SVC signal 唤醒（observed==0x8）三路全绿才进稳态打 'W'；waker2 内核任务经真 SVC 链（clock_read→thread_sleep 3 slice→signal 内核侧表 handle→再睡 1 slice）打 `WAIT ok`，此后 exit；同一 Event 双表挂同一 ObjRef（bootstrap 表 + waiter 表）——signal/wait 各持己表 handle 的跨表互通活证；**4-2a 验收：user_pinger（打 'p'）/user_ponger（打 'P'）对打**——boot 手搭 channel 对（两表各 insert 一端 + pair 互钉，pinger 持 endA SEND / ponger 持 endB RECV）；pinger send 灌到 -EBUSY → `wait_one(WRITABLE, now+10s)` 阻塞 → ponger recv 腾位唤醒 → 补发成功 → 首圈打 `CHAIN ok`（smoke 第 16 标记）后稳态每圈 'p'；ponger recv/-EAGAIN → `wait_one(READABLE)` 阻塞 → pinger send 唤醒 → 验 PING 载荷打 'P'——「send 满 → 对端 recv 腾位唤醒」两进程阻塞链活证（ktest 同步加 I6 低半拒面 + I7 Channel poll 面全套；Event 位面/I 段/waiter/waker2 全量迁高 32 位用户半）；**4-2b 验收：ponger 稳态等待改 wait_many n=2**——[endB:READABLE, boot 直插哑 Event:1<<32 永不 signal]，命中须 `*idx==0`（哑 Event 位永不置、idx==1 即断链自陷），首次打 `MANY ok`（smoke 第 17 标记；哑 Event handle 0x0001_0002 boot 断言定值——ponger 模板硬编码依赖）；ktest I8 盖错误面+poll 面（n 界/坏 handle/poll -EAGAIN 且 idx 留哨兵/远古 deadline -ETIMEDOUT 不睡/命中 idx 两向），B 段未实现样本 0x05→0x33；**4-3a 验收：POLICY C 用例断言升级**（eff=32 者独占出选——严格优先机器断言，见阶段沿革 4-3a 段）；**4-3b 验收：pi_client(32)/pi_server(0)/pi_probe(0) 三内核任务对打（全真 SVC）**——client send×8 + 第 9 条 -EBUSY 后 wait WRITABLE 睡（donor）、server 睡 Event E（1<<35）时 block 钩子重推成边（server 受捐 32）、probe 睡 5 slice 醒来断言捐赠字段后 signal E 放行打 `PI ok`（smoke 第 18 标记），server 醒（冻结 32）recv×8 腾位唤醒 client、client 补发自证后双双 exit；ktest J 段（`PI-K ok`）盖单跳判别/rights 门控/链式/环收敛/轮帽截断/哨兵跳过/冻结回落/共享表粒度（详见阶段沿革 4-3b 段）；`just run-el2` 同（EL2→EL1 eret 降级路径）。MMU 开（高半 TTBR1 内核表 `KERNEL_VA_BASE`，**2b-1 起内核主体高半执行**、异常 dump PC 应为 `0xffffff80…`；**2b-2 起低半身份表退役**——TTBR0 常态为空表或用户表，低半访问必 fault）、buddy/slab 堆、异常向量（current-EL + lower-EL 双路）+ panic dump、GICv2、SVC syscall（EL1/EL0 双入口 + 语义层注册 + copy_from/to_user 界检，24 号实现——0x04 阻塞化 wait（4-2a 扩 Event|Channel）/0x05 wait_many 随 4-2b/0x06 signal+唤醒+位空间检/0x54 event 随 3-4c+4-1、0x37 thread_sleep 随 4-0）、内核线程 + 上下文切换 + tickless idle + IRQ 抢占（含 EL0 运行中抢占）+ 切换遥测环（四切换位 yield/preempt/exit/block 写静态环，零分配）+ 唤醒遥测环（4-0 配套：双唤醒位 Signaled/Timeout 写 `WAKE_RING`，block→wake 延迟数据源，零分配）+ 睡眠/唤醒原语（4-0：SLEEPQ + block_current/wake_matching + tick 扫超时；4-1：wait_one 阻塞化 + signal 遍历唤醒；4-2a：Channel 唤醒点——send 成功唤对端 READABLE / recv 成功唤对端 WRITABLE；4-2b：WAIT_ANY 哨兵——多对象等待者任何唤醒都命中、醒来重验）全通。smoke 18 标记（`POLICY ok` 第 11 / `BLOCK ok` 第 14 / `WAIT ok` 第 15 / `CHAIN ok` 第 16 / `MANY ok` 第 17 / `PI ok` 第 18）+ switches 金丝雀严格递增。

**前置（macOS，一次性）**：

```bash
brew install qemu just                          # qemu-system-aarch64 + just
rustup target add aarch64-unknown-none          # stable，拉预编译 core（无需 nightly/build-std）
```

**日常命令**（仓库根；justfile 已固化 `~/.cargo/bin` 到 PATH）：

```bash
just build          # = cargo build（内核 ELF，target=aarch64-unknown-none）
just run            # QEMU virt 冷启动 → 串口打印 1BerryOS boot（默认起始 EL1），halt
just run-el2        # -machine virt,virtualization=on → 21BerryOS boot（强制 EL2→EL1 eret 降级路径）
just run-release    # release 构建 + 同样 QEMU
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增；SMOKE_KEEP_LOG=1 留日志
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just check / just clean
```

要点 / 已踩坑：

- **产物不在 `./target`**：本机全局 `~/.cargo/config.toml` 设 `target-dir=~/.cargo-target`（sccache + 共享 target）。路径解析收敛在 `工具链/paths.sh`（单一事实源，justfile/smoke.sh/CI 同源，内部 `cargo metadata` 动态取真实路径），无需关心。
- **QEMU ≥11.x 不接受 `-bios none`**（报 `Could not find ROM image 'none'`）；`virt -kernel <ELF>` 直加载即可，故 justfile / `.cargo/config.toml` 都不带 `-bios`。
- **QEMU halt 后不退出**：内核 `wfe` 死循环，串口打印完即停在那。退出 QEMU 用 `Ctrl-A` 然后 `X`（macOS 无 GNU `timeout`，自动化里用后台 + `kill`）。
- 调试：`-s -S` + lldb 单步；`-d cpu_reset -D qemu.log` 看启动 CurrentEL。
- 测试入口：`just smoke`（QEMU 无人值守冒烟，CI 同款，`工具链/smoke.sh`）+ `just test-host`（host 单元，不依赖 QEMU）+ QEMU 集成（`ktest` 框架，见 `文档/测试策略.md`）。

## Claude 在本仓库的工作方式

1. **先查参考**：涉及某子系统（IPC、调度、页表…）时，先看 `参考源码/` 是否已有对应对照项目，参照成熟实现再写。
2. **小步提交**：OS 调试极难，每个改动尽量小且可独立验证；能在 QEMU 跑通的里程碑单独成一个提交。
3. **不污染内核态**：内核 `#![no_std]`，禁用 `std`/`alloc`（在自有分配器就绪前）；用内核自有的分配器/日志；新增 crate 前确认 `no_std` 兼容。
4. **协议洁净**：主参考是 **Rust OS**（Redox / Theseus / zCore，MIT 可对照）；hubris = **MPL-2.0 语义可借、文件不搬**；rCore-Tutorial = GPL 只看思路；参照 seL4/Zircon/Linux（C/C++/GPL）时只看思路、不复制实质代码。本仓库许可证 Apache-2.0（ADR #37）。
5. **架构无关优先**：新功能先写成架构无关 Rust，确实需要特权指令时再落到 `架构相关/aarch64/` 并留好接口。
6. **博采众长不杂糅**：中心 = **可注入的「机制/策略分离」+ 硬件信任边界**；借鉴任何参考前先过 `架构综合纲领.md` 的**排除 8 问**（轴 PICK？冲突中心？Theseus 仅进程内？IPC 不统管？seL4 只锁不变量？hubris 静态只借 boot 层？GPL 不进？哪关 G0–G5 验证？）。
7. **改了约定就更新本文件**：新增目标架构、改变语言边界、确定构建命令时，同步回写 `CLAUDE.md` 与 `AGENTS.md`。

## 约束与陷阱（Gotchas）

- 内核 `#![no_std]`：禁 `std`/`alloc`（分配器就绪前）；`panic=halt`（自实现 panic handler + 栈回溯）；**浮点寄存器默认不保存**，用 FPU 的路径需显式保存恢复。
- **`unsafe` 纪律**：只在页表/上下文切换/MMIO/分配器/硬件原子用 `unsafe`，包薄安全层；绝不散落。
- **syscall 边界无 panic**：跨 syscall 的 Rust 函数返回 `Result`/状态码，绝不 panic（内核态 panic = 系统停）。
- ARM64 下注意 Cache 一致性、屏障指令（`dmb`/`dsb`/`isb`）、大端小端（默认小端）。
- **QEMU aarch64 `virt` 起始 EL 随配置而变**：默认 `-machine virt`+`-kernel` ELF 起始 **EL1**（实测 `just run`→`1`）；`-machine virt,virtualization=on` 起始 **EL2**（`just run-el2`→`21`，走 EL2→EL1 eret）。boot 第一行（`global_asm!`）`mrs CurrentEL` 检测并按需降到 EL1（关 MMU/cache、写 `sctlr`、`eret` 到 EL1）——**别假设固定 EL**。
- **PAC/BTI/MTE 目标 day-one 开**，但 Phase 0/1 推迟——`cortex-a72`（ARMv8.0）无此扩展；**v2** 切 `-cpu max` 时开 target feature（源码零改动，见 `安全与硬件隔离设计.md` §3/§6）。
- **QEMU TCG 下 CNTV 实测 tick 率 ≈ SLICE_HZ/2**：`CNTFRQ_EL0`=62.5MHz、`SLICE=FREQ/SLICE_HZ`=6.25M（配置正确，`SLICE_HZ=10` 应得 10Hz），但 `just run` 实测抢占 ~5Hz（200s 稳定 1000 切换、60s 稳定 300）——`CNTVCT` 在 TCG `virt`/`cortex-a72` 下推进速率 ≈ `CNTFRQ/2`。**非内核 bug**：真机或加 `-icount` 应为 10Hz。看 tick 率别被这 2x 误导。
- **2b-1 起内核主体链在高半（VMA = `KERNEL_VA_BASE` + LMA 镜像）**：符号/PC 显示 `0xffffff80…` 而镜像实加载在低半（QEMU `-kernel` ELF 按 **p_paddr** 落内存）——看布局跑 `python3 工具链/size.py --assert-flip`（smoke 已固化断言）。**ADRP 只有 ±4GB、双向坑**：低半早期代码（`.text.boot`）引用不了高半静态量（故 4 张 boot 表放 `.bss.boot` 低半）；高半代码取低半绝对符号地址同样越界（故 mm.rs 导入高半 `__kernel_end` 运行时减基址）。
- **2b-2 起低半已退役**：MMIO（GIC/UART）一律走高半别名（gic.rs/uart 内部常量 `KERNEL_VA_BASE + PA`），**PA 直访必 fault**——只有 boot asm 的早期打印例外（开 MMU 前直写 PA）。内核空表 `NULL_L1` 放普通 `.bss`（高半 VMA），TTBR0 写入按 `VMA - KERNEL_VA_BASE` 折算 PA。
- **用户页表叶必须 nG=1，全量失效用 `tlbi vmalle1is`**：`tlbi aside1*` 按号失效**够不着 global 项**（boot 块描述符全 nG=0）——用户叶不标 nG 则同 VA 映射跨 ASID 经 TLB 泄漏；退役/回卷要清 global 项只能全量 `vmalle1is`（`aside1all` 不是真实指令，设计文档 §5.2 原写法已更正）。
- **2b-4 起 EL0 异常走 lower-EL 向量 0x400（sync）/0x480（IRQ）**，帧宏 `save/load_frame_el0` 与 current-EL 版**仅 `sp` 槽差异**（存取 **SP_EL0** = 用户 SP；异常自 EL0 入 EL1 硬件强制 SPsel=1，帧仍落内核栈）。不变量：**EL0 运行期间 SP_EL1 留着本任务内核栈位置**（trampoline/上下文切换装入），lower-EL 异常入口才能精确落回该栈。
- **3-1 起 syscall 错误一律错误码自理、永不杀调用者**：未知/越界/未实现编号 → `-ENOTSUP`，坏 handle → `-EBADF`，EL0/EL1 同权（ABI §8.0「永不 panic」承诺的内核执法；2b-4「畸形号杀」临时政策已撤——杀只留给 CPU 故障：D-abort/I-abort/BRK 走 `user_fault_exit`）。用户页 AP=0b01 且未设 UXN = **用户页当前可执行**（NX/只读档留 v2）。
- **内核字节串 `b"..."` 只能 ASCII**：塞中文直接编译错，且下游 crate 可能耗旧 rlib 报出**误导性错误**（如 "cannot find xxx in crate berry_sched"）——见到莫名符号缺失先查上游 crate 真实编译输出；中文文案用 `println!`/`uart_puts(str.as_bytes())` 路径。
- **2b-4 起时间片按周期数记账**（`slice_left` 存 CNTVCT 周期、tick 按增量扣减）：TCG 的 2× tick 率效应原样继承（观测仍 ~5Hz/slice），行为与 tick 计数版一致。
- **`FrameAllocator::alloc()` 返回 PFN 不是 PA**（PA = pfn << PAGE_SHIFT），且帧内容不保证干净（buddy 只管元数据）——新页先清零再用。
- **3-2 起映射中 VMO 的「销毁性 close」拒 -EBUSY、别名 close 放行**：只有 refs==1（本次 close 会 drop 对象）且 mapcnt>0 才拒；dup 出的别名 close 不死对象、正常放行——写成「映射中一律拒」会把合法别名链（RO dup→close RO）误伤（3-2 调试期实踩）。decommit 同理拒映射中页；`Vmo::Drop` 对映射中页只泄漏。
- **syscall 主体不可抢占 = v1 免锁根基**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且 handler 全程不解蔽 → 引导表/对象访问单核串行；**任何新 syscall handler 不得解蔽 IRQ、不得忙等跨任务**（3-3 拆 per-process 表前都靠这条）。
- **`vmo_create` v1 上限 16MiB（`VMO_MAX_SIZE`）**：巨 size 的 `Vec` 分配 abort 是 EL0 可触发 panic（违 ABI §8.0），故超限先拒 -ENOMEM——新 syscall 凡按用户 size 做分配的都要照此设上限。
- **user_good 的 3-2 验收链是一次性的**（进稳态循环前跑一遍）：14 位代号经不起循环 create/close 耗 gen；改链时记住 `movz x3,#0x2018` 槽基会被 commit 步的 len 参数借走，须重载（曾因此把 pattern 写进自己代码页 0x1008——恰好自洽没炸，纯运气）。
- **spawner 内嵌子代码的 `movz x4, #<len>` 必须精确等于子模板字节数**（3-4b 起 324 = 81 指令）：改子模板后重数指令（每行字符串 = 1 指令 ×4B）再改常量——拷多带尾随零垃圾、拷少截断尾部，两种错都静默（vmo_write 只管拷不管语义）。ktest 的 `svc6` 是 nr+6 参共 7 形参——5 参 syscall（channel_send/recv 等）末尾要补 0 占位（3-4b 实踩 22 处）。
- **用户模板 svc 后复用参数寄存器，先想清值是什么**（3-4a 实踩）：派发只回写 x0、x1..x6 保留的是**你传进去的参数值**——vmo_read 后拿缓冲地址要用 x2（buf 参数），拿 x3 得到的是 offset（=0，野读被杀，watchdog `杀数 2 > 1` panic 拦下）。套路口诀：哪个寄存器装的什么，svc 前后一个样。
- **3-3 起内核对象间跨界引用必须 `ObjRef::retain/release` 钉住**（Thread 钉 Process、Process 钉 root Vmar、运行中任务钉 Process）：只存裸指针 = 父 handle 关光后 use-after-free。Drop 链（close→drop Thread→release Process→drop Process→release Vmar）纯指针操作不回表，无重入。另一边界：**ktest 在 `berry_sched::init` 之前跑 → 任何会 spawn 的成功路径（如 process_start）不能在 ktest 验**，只能盖参数/归属/状态错误面，happy path 交 EL0 用户进程 + watchdog 错杀断言兜底。
- **用户模板可用 `adr` 取同页内嵌数据**（3-3 spawner 的子代码字节）：自相对寻址整块搬迁后偏移不变，与「仅局部数字标号 + movz 绝对地址」的拷贝安全约定同源——但仅限**同页**内（adr ±1MiB 且不得跨模板边界引用）。
- **ktest B 段「未实现真号」样本是 0x33（process_exit）**：0x04/0x06/0x54 已随 3-4c 实现、0x05 已随 4-2b 实现——再拿旧号当样本断言 -ENOTSUP 会真 FAIL；后续实现 0x33 时（进程退出路径批）记得换下一个未实现号（0x35/0x36）。
- **`object_wait_one` 4-1 起阻塞化**：poll-first（命中即返不睡）→ 未命中且 deadline≠0 挂 SLEEPQ 睡到绝对时刻；醒来**重验**（醒因≠结果，清位竞态再睡循环兜底）；到点未命中 -ETIMEDOUT、`deadline==0` 仍是 poll（-EAGAIN）。命中回写**全满足集**（current，非请求子集）；等待不需权利位（12 位 rights 表无 WAIT 位）。`object_signal` 仅 Event、置位后 `wake_matching`（匹配键 = ObjRef 地址——同一对象跨表 handle 互通）。**睡眠钉住纪律**：wait 全程 `ObjRef::retain` 一次、唯一出口 release——睡眠期间全表 handle 被关光也不销毁；逐轮 retain/release 会在归零后再循环时 UAF（跨表 waker 场景真实可达）。
- **4-2 起信号位空间分半**：低 32 位 = 内核派生（`SIGNAL_READABLE`/`SIGNAL_WRITABLE`，Channel wait 面）、高 32 位 = 用户（Event `object_signal` 专用）。`object_signal` 的 clear/set 掩码**只收用户半**——越出 `SIGNAL_USER_MASK` 一律 -EINVAL（检在 get_raw 之后：坏 handle/类型仍 -EBADF 优先）。**清零掩码用用户半全集 `0xFFFF_FFFF_0000_0000`**（旧 `!0` 清零 4-2 起非法）；模板/ktest 用户位一律 `1<<32` 起编（U1/U2/U3=1<<33/34/35）。wait 低 32 位请求 Channel 上 READABLE/WRITABLE 之外的位 = 永不命中（同 Zircon 不设门）。
- **用户模板尾部内嵌数据（`.ascii` banner 等）后必须 `.balign 4`**：`UserProcess::build` 断言模板长度 4 字节倍数——banner 9 字节直接 KERNEL PANIC（boot 停在 ktest done 后，4-2a 实踩；banner 长度恰为 4 的倍数也保留该行——纪律一致性）。
- **多对象等待挂 `WAIT_ANY` 哨兵、不挂真实对象地址**（4-2b）：Task 只有单一 `wait_obj` 匹配键，`wait_many` 睡眠传 `berry_sched::WAIT_ANY`（usize::MAX）+ 全位掩码——`wake_matching` 谓词对哨兵等待者「任何对象、任何新置位」都命中，醒来由 syscall 层重验循环定夺（虚假唤醒无害）；**纯清位（set_signals==0）不误醒哨兵者**（交集为空）。数组出参 `*idx` 预清 u64::MAX = 未命中哨兵值。
- **boot 直插 handle 的定值可被用户模板硬编，但 boot 必须断言**（4-2b ponger 哑 Event）：ponger 表第 2 插 = 槽 2/代号 1 = 0x0001_0002 硬编进模板，main.rs `assert_eq!` 拦表序漂移——错值 → wait_many -EBADF → 自陷被杀 → MANY 缺失 = smoke FAIL（断言让漂移在 boot 期炸响而非静默错 handle）。
- **遥测环/切换钩子写入零分配**（`telemetry.rs` 静态预储环 + `sched.rs` `note_switch`）：三切换位是 IRQ 屏蔽热路径——try 化批堵住的 OOM-panic 面不得从这里重开；新钩子须「零分配、零切换、短平快」。`duplicate` 到**零权利**会被表拒（-EACCES，空 rights 拒面）——要「只读观察」handle 得 dup 到某个非零权利位（如 DUPLICATE）再靠「wait 不查权利」观察对象。
- **`block_current` 续行时 IRQ 仍屏蔽**（4-0）：切回点在唤醒方屏蔽窗/IRQ 窗口内，之后无自动解蔽点——**SVC 语境勿解蔽**（eret 自然恢复 SPSR，syscall 主体不可抢占免锁根基）；内核任务语境续行后自行 `enable_irq`（与 `yield_now` 的 disable→switch→enable 对称包络同款纪律）。
- **`SleepQueue::unlink` 不幂等**（4-0）：非环形双向链表无法廉价判「在队上」（单元素节点 prev/next 均 None 与空节点无异）——对不在队者调用会误清 head/tail 扬掉整队；三个调用点（block/wake/expire）都由屏蔽窗契约保证在队。
- **带内核派生位（READABLE/WRITABLE）的等待 ⟹ wait_obj 必须是真 channel ObjRef**（4-3b）：`pi::donor_edge` 靠这条不变量直接解引用 wait_obj——4-0 合成对象 SLEEP_OBJ（u8 静态、不透明 token）曾以 `!0` 位睡它（含派生位），PI 重推解引用 1 mod 8 地址即对齐 panic（run 实跑抓到）。合成/非对象等待一律只等**用户位**（内核永不置位；现役位：1<<32 哑 Event、1<<33 SLEEP_OBJ、1<<35 U3/E——新增前先查碰撞）。
- 微内核里"把东西塞进内核"是反模式 —— 先问：这能不能做成用户态服务？
