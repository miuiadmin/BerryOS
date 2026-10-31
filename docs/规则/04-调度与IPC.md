# 04-调度与IPC-调度器与 IPC

> 一句话定位：动 调度器/睡眠队列/IPC/信号位/PI/SMP 锁 前先读本文。判例全文 → docs/雷册-判例库.md（瘦行行尾锚点直达）；设计文档见文末「深读」。

## 域铁律

### 调度器与电源

改动调度 / 电源相关代码前，先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；调度策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：统一手机(大小核)→PC(全对称)→手表(单核小算力)——同一路径三端通用、端无主次；拓扑由 `arch/<arch>/` 填表，核心逻辑架构无关。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断——续航第一杠杆，非后期补丁。
- **每核 runqueue + 负载均衡**：多核吞吐主力；单/少核平台近零开销。
- **电源管理是用户态 Rust 服务**：DVFS / 热管理 / 休眠策略推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 消息拷贝）；PM 服务可崩，内核降级保活、监督重启。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电（tickless/idle/PM）与多核性能同为 v1 一等公民，路线见 M1a/M1b。
- **intent 两段式（ADR #42）**：**公平是机制默认，intent 是策略输入，叠加永不替代**。v1/v2 intent = u8 查表键，`berry_intent` 编码冻结 0x00 UNSPECIFIED–0x05 MAINTENANCE(冻结不启用)；M4 起 class_fair ×4 带实例 + 预算包络 + deadline-server 兜底，**凭 v1 遥测入场券**；band/intent **不入 AI 动作空间**。

细节 → `docs/调度器设计.md`。

### 现役用户信号位登记表

用户位只住高 32 位（低 32 位 = 内核派生 READABLE/WRITABLE，见「判例瘦行」§A29 行），一律 `1<<32` 起编、集中登记 `kernel/src/main.rs` 的 `USER_SIGNAL_BITS` 表（编译期去重断言执法）：

| 位 | 占用者 |
|------|------|
| `1<<32` | 哑 Event（永不置位的对照项） |
| `1<<33` | SLEEP_OBJ |
| `1<<34` | ktest·AI_EV_DONE |
| `1<<35` | U3 |
| `1<<36` | E_rel |
| `1<<37` | U2 |
| `1<<38` | U4（握手专用，清位豁免） |
| `1<<39` | RX Event |

新增用户位前先查本表碰撞；合成/非对象等待只等用户位（内核永不置位，见「判例瘦行」§A40 行）。口径源 = §A29 / §A40 / §A61（模板握手信号用专用位、清位豁免——§A61 瘦行正主在 `06-用户程序与编译链.md`）。

## 判例瘦行

- **IRQ 屏蔽窗契约（4-0，调度睡眠队列）**：①`block_current` 续行时 IRQ 仍屏蔽（切回点在唤醒方屏蔽窗内、无自动解蔽点）——SVC 语境勿解蔽（eret 自然恢复 SPSR）；内核任务续行后自行 `enable_irq`（与 `yield_now` disable→switch→enable 对称）；②`SleepQueue::unlink` 不幂等——对不在队者调用误清 head/tail 扬整队；block/wake/expire 三调用点靠屏蔽窗保证在队；③`for_each_sleeper` 须屏蔽窗内调用——tick 超时摘队会动睡眠链接、遍历中接线即扬队；任务体经 SVC eret 后 IRQ 已解蔽，照 ai_probe 手动 disable/enable 包络扫描窗。先问：续行点谁负责解蔽？调 unlink/遍历睡眠链前有屏蔽窗包络吗？ → 雷册 §A38/39/47/§B10
- **内核任务勿睡共享表（4-5/4-6）**：PI 受捐按持有表反查——共享 `BOOTSTRAP_HT` 睡眠者视作持有表内一切 handle，错捐级联可 prio-0 饿死；被错捐者自己每轮严格优先抢片、把断言对象推过自身重试预算。修法 = 常驻服务/步进重试司机一律私有表：`Box::leak` + `spawn_with_tag(entry, arg, name, prio, 表指针)`（tag≠0 走私有表、tag==0 ktest/boot 零扰动；表内仅 E_rel 一个 Event = 零受捐面）+ 预算按「最坏轮转 + 自己也偷片」放宽（64）；自证 = `task_svc_tag` 双断言（私有表 -EAGAIN / 共享表 -EBADF，`TAGTAB ok`）。halt 后「全系统冻结」先查 panic 尾巴。先问：这个睡眠者睡共享表了吗？预算含自己偷的片吗？（§B9 并入：实测形态 = 被错捐 eff 32、自己每轮抢片；probe 类步进任务同挂私有表。） → 雷册 §A45-46/§B9
- **`object_wait_one` 阻塞化（4-1）**：poll-first 命中即返；未命中且 deadline≠0 睡 SLEEPQ 至绝对时刻，醒来重验；到点 -ETIMEDOUT、`deadline==0` 仍 poll（-EAGAIN）；命中回写全满足集（current 非请求子集）；等待不需权利位；`object_signal` 仅 Event、`wake_matching` 键=ObjRef 地址（同对象跨表 handle 互通）。睡眠钉住：全程 `ObjRef::retain` 一次、唯一出口 release——全表关光不销毁；逐轮 retain/release 归零再循环即 UAF。先问：retain/release 唯一成对吗？ → 雷册 §A28
- **信号位空间分半（4-2）**：低 32 位=内核派生（`SIGNAL_READABLE`/`SIGNAL_WRITABLE`，Channel wait 面）、高 32 位=用户（Event `object_signal` 专用）；clear/set 掩码只收用户半，越出 `SIGNAL_USER_MASK` 一律 -EINVAL（检在 get_raw 后，坏 handle/类型仍 -EBADF 优先）；清零掩码用用户半全集 `0xFFFF_FFFF_0000_0000`（旧 `!0` 非法）；模板/ktest 用户位一律 `1<<32` 起编；wait 低 32 位请求 READABLE/WRITABLE 外的位永不命中。先问：掩码越出用户半了吗？ → 雷册 §A29
- **多对象等待挂 `WAIT_ANY` 哨兵、不挂真实对象地址（4-2b）**：Task 只有单一 `wait_obj` 匹配键，`wait_many` 睡眠传 `berry_sched::WAIT_ANY`（usize::MAX）+ 全位掩码——`wake_matching` 对哨兵者「任何对象、任何新置位」都命中，醒来由 syscall 层重验循环定夺（虚假唤醒无害）；纯清位（set_signals==0）不误醒哨兵者；数组出参 `*idx` 预清 u64::MAX = 未命中哨兵值。先问：多对象等待挂的是哨兵还是真实地址？ → 雷册 §A31
- **PI 共享表粒度级联饥饿（4-5a）**：proc_tag=0 内核任务睡共享表 → 睡 channel 派生位（READABLE/WRITABLE）即 donor，受捐覆盖所有持 peer 端点的睡眠者；叠跨层严格优先可把 prio-0 常驻任务永久饿死。新常驻内核服务两道防线都要上：① `spawn_with_tag` 挂 leak 私有表；② 稳态等待勿 wait_one 直睡 channel 端点——wait_many（WAIT_ANY 哨兵 → `pi::donor_edge` 返回 None）+ 永不置位哑 Event 对照项退出 donor 面；高优 client 阻塞服务满的真实受捐不受影响。先问：常驻内核服务睡了共享表或 channel 派生位吗？ → 雷册 §A34
- **`wake_matching` 返回计数勿当收工判据（4-2b 哨兵起）**：WAIT_ANY 等待者匹配**任何**对象的唤醒——返回值混入误醒票不再可信；waker 类任务收工判据用被唤方置的旗标（sleeper 侧 `BLOCK_BOOT_DONE` 模式）。先问：收工判据靠返回计数还是被唤方旗标？ → 雷册 §A35
- **带内核派生位（READABLE/WRITABLE）的等待 ⟹ `wait_obj` 必须是真 channel ObjRef（4-3b）**：`pi::donor_edge` 靠这条不变量直接解引用 wait_obj——合成对象睡它（含派生位）即解引用 1 mod 8 地址对齐 panic；合成/非对象等待一律只等**用户位**（内核永不置位）；现役位集中登记在 `kernel/src/main.rs` 的 `USER_SIGNAL_BITS` 表（`1<<32` 起编），新增用户位必先登记该表（编译期去重断言执法）。先问：这个新用户位登记进 `USER_SIGNAL_BITS` 了吗？ → 雷册 §A40
- **intent 传播的数值序（4-3c）**：u8 越小档越高（0x00 UNSPECIFIED 顶），「借入取 max」= 数值 `min`；每消息重导 `eff = min(declared, msg_intent)` 非单调，勿按历史 max 累积；三道帽超顶降为不传播（非 kill）；转发降级≠自身回落：slow 超预算 send 装自己 declared、自身 `intent_effective` 保持，仅 recv 结算改档；`channel_create` 4-3c 起 options 首参，旧签名静默错位；ktest 超帽样本=伪造消息直压收端队列；`Task::new_kernel` 返回 `Box<Task>`。先问：结算按历史 max 累积吗？ → 雷册 §A41
- **fastpath 检查链含「全队无哨兵」链项（4-4）**：`find_single_waiter` 遍历中撞到任一 WAIT_ANY 等待者立即 None——哨兵者被任何非零唤醒命中，单点唤醒集合 ≠ `wake_matching` 全匹配集，快路漏醒哨兵 = missed wakeup；多匹配者同理退 slowpath（唤醒集恒等 =「不长第二套」执法点）。先问：快路唤醒集合与全匹配集恒等吗？ → 雷册 §A42
- **`try_preempt` 严格更高才切（4-4）**：直切=选者 `prio_effective` 严格高于当前——同层 FIFO 不因 IPC 插队；协议同 `block_current`（屏蔽窗内可切走、切回续行、IRQ 勿解蔽），授片即 `arm_slice` 重编。幻影出选防线：`pick_next` 非纯查询（选中即 `advance_past` 耗 head）——「先 pick 再判」白耗 head、锁死出选相位 → 结构性饿死；故以 `Runqueue::has_eff_above` 只读探峰前置，不可能切就不 pick；「pick 了又不用」同罪（tick `next==cur` 自耗合法——cur 真被服务过）。先问：会不会 pick 了又不用？ → 雷册 §A43

## 深读

- docs/调度器设计.md —— 机制 + M 里程碑权威：runqueue/抢占点/tickless/intent 两段式与省电双线的完整设计。
- docs/IPC设计.md —— channel/Event/信号位与 object_wait/wait_many 协议的第一手语义。
- docs/AI调度设计.md —— AI 干涉面现行口径：限带内、机制预留 + 实验轨道 + kill criteria（ADR #21 修订版）。
- docs/SMP设计草案.md —— S 系拆步与锁序/BKL 出路的现行依据（SMP 锁改动前必读）。
