# G3-5 侦察档：转正候选① 常驻再装形 lat——respawn 僵尸累积 vs pages 稳态窗冲突定谳

> 刀：G3-5 转正候选① 侦察（草案 §8 G3-5 段 E 后继刀候选①）。
> 产出 = 冲突面定谳记档 + 可行/不可行裁决 + 后继刀形状。日期：2026-11-05。
> 性质：纯侦察零代码零验证面（同工件不重跑——evidence-g3-6.md 树一致性先证）。

## 问题

lat 载荷现为 boot 窗一次性（每路 ×32 样本，测完 exit_current——rt_probe 同形），
尾延迟帽只覆盖 boot 窗至首拍前；稳态时延面能否用真 BENCHLAT 数字覆盖？
候选形 = 「周期重启采样」。冲突面 = 舰队 respawn 每周期的内存残留是否撞
pages 稳态窗（soak-g3.sh:183：`[pbase, pbase+64]` 双向窗，第 7 拍起真比较）。

## 事实链（全 file:line 锚点，主树 d3e42e0）

### 僵尸累积面（核心）

1. **A45 有意泄漏 = 判死主证**：内核任务 pinned==0 退出钩子**直通不 drain
   私有表**（kernel/src/main.rs:1494 + 4179-4182 lb_master 注释明言
   「**A45 有意泄漏**，par/rt/soak 同款」）。latsgi waker（task_latsgi_wak）
   同款：表内 fleet Process + code VMO + data VMO 句柄随任务退出**永不关**。
2. **storm_spawn_fleet 每周期足迹**（kernel/src/main.rs:4793+）：1 Process +
   code VMO 1 页 + data VMO (2+writer_n) 页（latsgi writer_n=1 → 3 页），
   svc 链全落 waker 私有表 → **≥4 committed 页/次 spawn** 被 A45 泄漏持有
   （外加 Process/AS 页表结构页，合计估 6-9 页/次）。
3. **进程侧收口不救**：舰队末线程 thread_exit → 末线程收口 drain 子表 →
   AS unmap 真还（kernel/src/main.rs:5052-5054 ALLOC-1 实证注释）——但那只
   drain **进程自己的表**；VMO 对象本体由 waker 表持有，unmap 不减其引用
   → 页仍拴在活 VMO 上。
4. **任务体本身不漏**：Task Box + 16KiB 内核栈走 REAPQ → CPU0 reap_drain
   真回收（sched/src/sched.rs:900-1010 退出尾部；main.rs:3777-3785 全回收
   注释）。泄漏面**只有**对象/表，无任务体。
5. **表也是 boot 单例**：waker 私有表 = boot `Box::leak`（main.rs:1806，
   spawn_with_tag 挂表）——周期 respawn 须每周期新表 Box::leak = 又一线性
   泄漏向量（内核堆页）。
6. **拍数实证**：银行跑 PASS **60/60 拍**（out/bench-receipts/
   soakg3-bank-stdout-20261105.log）——A 形线性累积 = 6-9 页 × 60 =
   **360-540 页 ≫ +64 窗**，第 8-15 拍即破窗。

### 回收路线在册

7. **ABI 0x01 NR_HANDLE_CLOSE 在册**（kernel/src/abi_gen.rs:11）——显式
   close 回收路线可用；但撞 **EBUSY 悬空 PTE 防线**（abi_gen.rs:116：映射
   未解除的 VMO 不能 close）→ 顺序敏感：须舰队末线程收口 unmap **之后**
   再 close（waker 须先等舰队全退再收自己的表）。

### 时序/测量面

8. **周期成本 ≪ 拍距**：两路各 32 样本全链在 boot 窗（首拍 60s 内）完成
   （银行跑 spawn 序：IPC 路 → soakmon → soakload master → SGI 路 waker
   全在首拍前）→ 单周期 ≤ 数秒级，60s 拍距裕量充足。
9. **一次性泄漏不可见性**：现行 boot 窗单发泄漏 ~6-9 页在首拍基线内吸收
   （基线 = 首拍值，比较全相对化）——这正是 A45 判例容忍短命 bench 任务
   泄漏的机理；周期化把「一次性」乘 N 即破。

## 裁决：三形定谳

| 形 | 形状 | 裁决 | 依据 |
|---|---|---|---|
| **A** | 周期 respawn，无回收（照搬现 spawn 链） | **不可行** | 事 1-6：A45 泄漏 6-9 页/拍 × 60 拍线性累积，第 8-15 拍破 +64 窗 |
| **B** | 周期 respawn + waker 显式 close 回收 | **可行·备选** | 事 7：0x01 在册；顺序敏感（收口 unmap 后 close，先等舰队全退）+ 每周期新表仍须复用单表 |
| **C** | **常驻再触发（零 respawn）** | **可行·最优** | spawn 恰一次进首拍基线（事 9 机理反用）；sleeper 常睡 SLEEPQ 零 CPU；无 mid-flight 快照竞速；BENCHLAT 周期行由常驻 waker 打 |

**裁定：候选① 可行——改形 C 优先（B 备选），A 判死。**

C 形相互面核验（全绿预期，实施刀复核）：

- **pages 稳态窗**：常驻舰队足迹恒定（首拍基线吸收）——零增量零竞速。
- **Δsw1 地板**：周期唤醒只加切换数——地板是下界，有利向。
- **park 帽**：sleeper 睡 SLEEPQ 非 park——无扰。
- **churn 计数**：soakload 自家计数器——无扰。
- **lockdep**：同锁路径 N 次重复——银行跑 60 拍 VIOLATION 0 先证。
- **SGI 路 spread→CPU1 与 soakload 写者共核受扰** = 稳态测量的意图本体
  （受扰时延正是要测的对象）。

## 后继刀形状（转正候选①实施刀，C 形）

1. **fleet/waker 常驻化**：boot 一次 spawn（现链不动）→ sleeper 采样毕
   **回睡**（不 exit_current）等下周期触发 → waker 常驻循环（触发 → 32
   样本 → 打 BENCHLAT 行 → 回睡）。触发源二选一（实施刀定）：soakmon
   拍后置共享 Event，或 waker 60s 对齐周期睡。IPC 路内核任务对同形改
   常驻（其本无 EL0/VMO——A45 泄漏面为零，改常驻纯为统一形态）。
2. **harness lat_face N 行收割改写**：每路 BENCHLAT 恰 N 行（N=拍数或
   周期数）+ LATIPC/LATSGI ok 计数恰 N + max-over-cycles 对帽；**首跑
   = CALIBRATE 观测**定稳态帽（boot 窗数字不可外推稳态——soakload 干扰
   随拍波动，帽须重标定）。
3. **常驻任务 A45 合规**：waker/sleeper 常驻 → 私有表 boot 单例即可
   （现形已合规，无新面）。
4. **kill criteria**：若 C 形首跑 pages 窗破（常驻足迹超预期）或稳态帽
   CALIBRATE 后 p99 超银行跑帽 ×3.0 量级 → 候选①降级，候选②（lockdep
   N 跑转正）顶上。

## 附：本侦察未做的事

- 未跑任何 QEMU/验证（侦察刀合同 = 纯读树定谳；同工件重跑零信息增量）。
- 未改任何内核/harness 代码——C 形实施归后继刀。
