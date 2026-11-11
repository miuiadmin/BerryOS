# SMP-S5 实施设计草案（G2 关卡收官——并发压测集 + 双核 soak 刀）

> 版本：**1.0**（v1.0）｜2026-11-06

> 母案：`docs/SMP设计草案-v1.0.md` §4 S-5 行 + §5 验收面。
> 出口：`docs/测试策略-v1.0.md` G2 行「待办」→「已过」+ G2 话术落 AGENTS（§7 四条限定对表）。
> 前置：S-4 全毕（五相 smoke-smp PASS + TLBS n=10 + 04feec2/2aaf8c2 落库）。

## §0 replan 记录（LoopX periodic review 2026-11-03）

- 后继 todo：`todo_8fe4a3028e11dd7aa05bc04f`（[P1] S-5 G2 关卡收官实施刀）。
- 六路侦察 workflow（lanes 1-6）+ 批评员十漏面 → 四大裁决 + 刀程六段（§2/§3）。
- vision 面继承 E1 收官 replan：S-0..S-4 毕，SMP 线唯一余刀 = 本刀；E2 逐域下放不占刀。

## §1 现状侦察结论（2026-11-03，基线 2aaf8c2）

### 1.1 G2 出口规格（冻结）

- `测试策略-v1.0.md` G2 行：`| G2 双核 SMP | 负载均衡对、TLB shootdown 对、无竞态 | -smp 2 + 并发压测 | 待办 |`
  ——验收项 1（LB 对）/项 2（TLBS 对）已在册（smoke-smp 相 3/相 5）；**项 3「无竞态」= 本刀主面**。
- G2 话术四条限定（§7 对表）：G2 ≠ G3；仅 QEMU TCG -smp 2（真机未验）；E0「内核串行/用户并行」
  话术仍现行；内核串行指锁域序，非单核化。

### 1.2 既有压测面盘点（可复用 vs 缺口）

| 面 | 现状 | S-5 处置 |
|---|---|---|
| canary 页 | S-4 4-4 已兑现（TLBS ok n=10，双核并发 map/unmap + canary） | 范围裁决：不重造，子案记指针 |
| 分配器完整性 | churn 终账恒等（:214-218）+ allocated_pages() 水位 | 相 6 双核 EL0 化（内核任务 ttbr0==0 恒钉 CPU0，不可用） |
| HandleTable | used_count() 仅计数，无结构走查 | 相 7 新写 integrity_check（§4） |
| 双核 IPC 压测 | 无 | 相 8 单进程双线程 ping-pong |
| soak | 单核 soak.sh，心跳 `sw= pages= lost=` | 独立 soak-smp 通道 + 心跳 v2（§2 D1/D4） |

### 1.3 per-CPU 统计缺口（D4 动机）

- `SWITCHES` 全局原子盖不住 CPU1 冻死（对核停摆它仍递增——单侧贡献）。
- 全仓无 idle 度量，「双核满载」不可证（批评员漏面 #4）。
- 测试策略 :83 idle residency 常设义务——最低限度兑现面 = per-CPU park 计数。

### 1.4 路由约束（载荷设计输入）

- worker 必须 EL0 thread_start（内核任务恒钉 CPU0）；`__user_*` 内嵌模板先例（tlbs_spawn_fleet）。
- **无 affinity syscall**（ABI 冻结 = 保留门 C）→ 双核铺面 = spread 路由
  （root process_start spread=false→CPU0 + thread_start spread=true→K=2→CPU1）。
- fw_cfg 门控缺席零扰动（bench-rt/soak 键先例形态）。

### 1.5 gen 预算（相 7 设计输入，§A22）

- 14 位 ≈ 16383/槽；耗尽槽 next_gen > GEN_MASK 合法驻留（alloc_slot 跳过）。
- 风暴轮数帽须设计在 slots × gen 余量内（churn_handle 32 槽 × 64 轮先例量级）。

## §2 四大裁决

**D1 双核 soak = 独立 `soak-smp` 通道**（新 tools/soak-smp.sh + just soak-smp）。
30min 塞 smoke-smp 使手动关卡过重；-smp 条款四处同步扩线
（smoke-smp.sh 头注 / justfile 两处 / AGENTS:98-99 / 规则07）。

**D2 满载载荷 = 内核门控 EL0 fleet**（tlbs_spawn_fleet 形态，fw_cfg `opt/berry/soakload`
双核常驻静默载荷）。不越基准文档 :401-402 用户态服务 churn 边界；不新增 user bins
（绕开 build-user 产链 + 标记计数三处翻档）；载荷零打印（panic SMP 安全面）。

**D3 压测四件 = canary 界定 4-4 已覆盖 + smoke-smp 新增相 6/7/8（独立 boot 专项门）**。

**D4 副核见证 = per-CPU 统计面进心跳**：sched 切换位点 per-CPU sw 计数 +
idle park 计数（SmpCpu 尾插字段）；心跳 v2 `SOAK t= sw= sw0= sw1= park0= park1= pages= lost=`；
宿主断言：sw0/sw1 各自严格递增（对核冻死即红）+ 满载核 park 增量有界（满载谓词）。

## §3 拆步（六段，每段独立可验证——S-2..S-4 同型）

1. **S-5a 基座**：本子案落稿；per-CPU sw/park 计数器（smp.rs 字段 + sched 六切换位点 +
   三 park 位点 + readout）；HandleTable::integrity_check；ktest H/H2 段。
   验证：build + test-host + unsafe 台账 --check + 主 smoke 零漂移 + 变异标定·走查株。
2. **S-5b 相 6 ALLOC + 相 7 HT**：双 fw_cfg 门控 spawn 链 + EL0 fleet + smoke-smp 扩相 +
   PASS 行/AGENTS/规则07 三处同步。验证：smoke-smp 全相 + 变异标定两株。
3. **S-5c 相 8 PP**：单进程双线程对打 + 序号零丢失对账。验证：同上 + 丢消息株。
4. **S-5d soak-smp 通道**：脚本 + recipe + 条款四处 + 30min 实跑
   （run_in_background 显式 timeout 7200000——默认帽 1800000 恰 30min 必超）+ R3 定标。
5. **S-5e 对抗审查收官**：五镜 workflow（S-4 4-4 形态）+ confirmed 修复回写。
6. **S-5f 文档收官 + 终证**：主 smoke 八跑终证 + 每新 fw_cfg 键四路零扰动实录 +
   G2 行转档 + G2 话术落 AGENTS + 总闸/索引 true-up + 沿革 + 判例 §A121 起 + ci.yml 议程落款。

## §4 验收面（G2 项 3「无竞态」全量）

- **相 6 ALLOC**：双核 EL0 churn fleet（vmo create/commit/write/close）join 后终账
  `allocated_pages() == 基线`（水位恒等，Relaxed 纪律）+ 双核参与证 → `ALLOC ok n=`。
- **相 7 HT**：双核 Event 风暴（零帧最省水位）→ 静默点（风暴前 + join 后）
  `integrity_check() == Ok(对账数)` → `HT ok n=`。走查不变量四条：
  ①槽 0 恒 Free；②Free.next_gen ≥ 1（> GEN_MASK 合法）；③Used.gen ∈ [1, GEN_MASK]；
  ④Used.refs ≥ 1（0 = UAF——并发竞态真病灶面）。零分配 panic-free，
  违规 FAIL 行 with_print_lock + 受控停机（不裸 panic）。
- **相 8 PP**：`PP ok a= b=` 计数谓词（§A32 禁竞速/吞吐时序）。
- **soak-smp ≥30min** 双核满载零断言失败（心跳 v2 全字段单调性 + 满载谓词 + 终账恒等）。
- **主 smoke 八跑零漂移** + 变异标定株系（走查株/泄一帧株/丢消息株——新断言必证可红）。
- 标记前缀查重：ALLOC/HT/PP ok 与在册六前缀无子串互撞（已核）。

## §5 全程纪律（批评员 + 判例收编）

1. §A32 状态谓词；§A116 with_print_lock 整段包络；§A117 有界帽轮询；
   §A118 deadline 双轨；§A119 新 VA 全仓双面净区扫描（避 mmap/pthread 区 + 已占 0x4800_0000）；
   §A44 ktest 睡眠者在队禁 SVC send；§A120 逐防线单断归因。
2. 基线快照在参与者 spawn 前（R0 前科：spawn 尾即发 SGI）。
3. 容量帽对表：PROCESS_MAX=64 / CHAN_HND_MAX=8 / 128MiB RAM 水位窗。
4. handle 家族 icount L2 内联级联（雷册 :443，handle_evt 166 基线）——漂移按 R2 配方归因。
5. reap_drain 竞速窗（sched.rs 无 KL 包络摸 buddy/slab）= 压测首个照到的镜子，
   对抗审查重点镜面（炸此处是真 finding 非压测 bug）。
6. CI 全链绿门槛每落码态过；unsafe 台账重产随源（integrity_check 新块已入册 :491）。

## §9.1 S-5a 执行追记（2026-11-03 落码，同日验证面六绿收官——详本节末）

- 落码面（本段）：
  - `arch/aarch64/src/smp.rs`：SmpCpu 尾插 `sw_cnt`@88/`park_cnt`@96（逻辑尺寸 104 取整
    128，size 断言不变；asm 只钉 0/8/16 三偏移故尾插安全）+ 偏移编译期断言 +
    `note_switch_cpu`/`note_park_cpu`（TPIDR_EL1 + base==0 防御早退 + 本核独占写）+
    `sw_park_readout`（跨核 volatile 读，S-5d 心跳 v2 消费）。
  - `sched/src/sched.rs`：六切换位点插 `note_switch_cpu`（yield/exit/block/tick preempt/
    dispatch_from_park_ctx/dispatch_from_idle——全在 IRQ 屏蔽锁窗内）；
    idle_loop 两 park 臂插 `note_park_cpu`（SMP None 臂 + 单核分支，wfi 前）。
  - `kernel/src/main.rs`：secondary_main 环尾 masked-wfi 前插 `note_park_cpu`。
  - `kernel/src/handle.rs`：`integrity_check` 落码（§4 四不变量 + 空表 Ok(0) +
    走查计数独立累计）。
  - `kernel/src/ktest.rs`：test_handle 增 H 段（混合态 Ok(2) + 空表 Ok(0)）+
    H2 段（全关后 Ok(0) 与 used_count 双归零）。
- 验证面：just build 绿（1.80s 零 error）+ test-host 128 过零挂 +
  unsafe 台账重产 --check 绿（新块入册）+ docs_guard PASS
  （顺手修 AGENTS:99 前账长行 147→98——HEAD 原样红，机械类②）。
- **变异标定三株**（尸留 /tmp/s5a-mut-red.log · mut2 · mut3 · clean-green）：
  - 株1 refs=0 出生：PANIC @ handle.rs:307（close 下溢）——不变量违例不可静默。
  - 株2 duplicate gen 越域（+GEN_MASK+1）：PANIC @ ktest.rs:151（SYSCALL close-dup）
    ——套件共享路径前测先炸，单点腐到不了 H（覆盖密度的意外实证）。
  - 株3 checker-lies（走查返 Err）：PANIC @ ktest.rs:2342 H 段
    `Err("MUT") ≠ Ok(2)`——**H 断言接线活**，走查红面直接证。
  - 净还原复证：裸 boot `HANDLE ok`（:15，零 PANIC）。
  - 走查独占检测窗 = 相 7 风暴静默点（boot 套件共享全部表变更路径——株2 实证），
    相 7 株（风暴残留态腐）S-5b 落。
- **主 smoke 零漂移：让窗待跑（一次撞窗实录）**——首轮放行依据 = hostfwd
  18081-3 稳定空闲 45s + 6c「链后只剩 smoke-image 零端口」口述；实况 = 6c 链
  （test-host→smoke-image→**just smoke 整跑**）第三段刚起，其 echod（host 侧
  18080 全程在席）拒我方 bind → 我方 smoke fail-fast 干净退场（前段标记全在、
  零漂迹象，未伤对方链）。教训入 qemu-field-protocol 记忆：判据须含 18080 +
  钉外层脚本 PID（QEMU PID 跨相位滚动）+ pgrep 实况优先于口述。
  重跑触发 = 6c 链（79997）整体退出 + 30s 稳定 + 场净双验后自动唤醒。
- **二次让窗实录**（watcher 触发后的竞窗皮）：bx7vgyfdj 报场净属实（18080/smoke 净），
  但放行前复核发现**新占用**——9534 `bash tools/smoke-image.sh` + 9558 QEMU
  （-pflash EDK2 + berryos.img + ramfb 形态，零 hostfwd 与四口无交集）恰在 watcher
  终验后的数秒竞窗内起跑。四口虽净，单字段协议（TCG 抢核）仍须让 QEMU 本体。
  处置 = 链式后台任务（等 9534 退出 + 任意 qemu-system 净 + 3×10s 稳定 + 20s 缓冲
  + 起跑前四口/场净终验 → 自动起 `just smoke` → 单通知带回退码）——省一个唤醒
  周期，且判据全程含 18080（echod 面）与 qemu-system 名匹配（pgrep 默认匹配
  进程名而非 cmdline，无自匹配陷阱）。
- **v1 终验拦下 + v2 升级**：v1 钉单轮 PID（9534）退出后 20s 缓冲窗内，
  **GUI-8 会话**（worktree `berryos-gui8draft` + 独立 target dir `~/.cargo-tmp-gui8-S3`，
  `just smoke-image` 终验面 → /tmp/gui8-efi-repro/）背靠背新轮进场——终验门
  `FIELD_NOT_CLEAN` 干净拦下（护栏起效）。归属判定实锤非 6c 链。v2 判据升级：
  不钉 PID，**全信号（qemu-system ∪ 18080 ∪ smoke-image.sh）连续 120s 静默**
  才占场 + 终验失败回环重等（≤5 試）+ 全量日志落盘 + 退码穿透通知行。
  GUI-8 面零端口零 target-dir 交集，唯 TCG 抢核须让（单字段协议全款）。
- **v2 终局：主 smoke 八跑 PASS（20:03:44 场净起跑，退码 0）**——金丝雀 43 条严格
  递增 + rootfs 相 47 全链 + RX 交互相 74 + FAT 子目录/持久化双相 + B4 掉电三 boot
  （毒环注入 + 恢复终验 clean）；**全程与 6c rig QEMU（53970）并行载荷下过**——
  时敏面裕度实证（对称风险预警不成立态；6c 同窗定谳回馈：其 smoke-image
  「缺 PAGE ok」纯载荷 flake 代码无辜，已修状态裁定判据）。S-5a 验证面六绿齐：
  build / test-host 128 / unsafe 台账 --check / docs_guard / 变异标定三株 /
  主 smoke 零漂移。

## §9.2 S-5b 执行追记（2026-11-03 相 6/7 落码）

- 落码面：
  - `kernel/src/user.rs`：`STORM_DATA_VA = 0x4900_0000`（§A119 双面净区
    实证——与 0x4800 同段，全仓双面零命中）+ `__user_allocst`（64 轮
    vmo create/commit/write/close churn）/`__user_htst`（event
    create/signal/close 风暴）双模板 + accessor。窗口 = SP 自推导
    （sp & ~0xFFF = 自身窗口页基——fleet 按线程错落 sp 派发窗口，解
    out-param 并发踩踏）；flag = **绝对 VA movz 合成**（0x4900 三面钉）。
  - `kernel/src/main.rs`：ALLOC/HT_PROBE_NAME + STORM_{WRITER_N,
    PARTICIPANTS,ROUNDS}（n=320 三面钉）+ ALLOC/HT_EV_BIT（1<<44/45
    入 USER_SIGNAL_BITS 登记）+ `storm_spawn_fleet`（data VMO 一炉
    (1+P) 页：flag 页 + 每人窗口页；root process_start 家核 + 4 写者
    thread_start spread→CPU1）+ `storm_win_read`（svc vmo_read 直读
    窗口）+ `storm_poll`（tlbs_poll 同形有界轮询）+ `task_alloc`/
    `task_ht` master + 双 boot gate 块。
  - `tools/smoke-smp.sh`：相 6/7 段（gate 自证行 + n==320 断言 +
    DEADLINE_STORM=240）+ PASS 行/头注/AGENTS:99/规则07 四处条款同步
    （五相→七相）。
- **三轮尸检定谳（首跑连环红——全为设计修正非内核病灶）**：
  1. **klock 窗内 svc 死锁**：storm_poll 首版把 storm_win_read（走 svc
     dispatch，自带锁窗）放在 klock_enter 窗内 → 重入死锁——task_alloc
     无声卡死（无 PANIC 无 ok、系统活、rt_ticks 走）。与 tlbs_poll 的
     关键差异：tlbs 观测面是原子计数（锁窗内读安全），storm 观测面是
     svc。修法：窗读移锁窗外（block_current 仍锁窗内——tlbs_poll 契约）。
  2. **flag 邻页推导仅 root 成立**：首版模板 flag = win-0x1000（邻页）；
     写者窗口 2..5 的前页是他人窗口页 → 读恒 0 永自旋（dones=64≠320，
     errs=0——root 一人满轮铁证）。修法：flag 改绝对 VA movz。
  3. **水位对账口径重设计（两跑连环）**：+24（boot 演示/常驻面分配混入
     对账窗）→ 加 boot 沉降稳定等待后 -6（终账前稳定等待把 REAPQ 栈帧
     drain 拉进窗 = 结构性 -20±）。定谳：**SMP EL0 形态全局水位无干净
     窗**（多源噪声 ±几帧 + 生命周期帧时移）。终态口径 = boot 沉降
     稳定等待（60s 帽）保基线净 + 终账**立即快照**（不等 drain——栈帧
     移交 REAPQ 后水位不动）+ 断言改**带宽 |Δ|≤64**（churn 全泄主信号
     ±320 量级，裕度 5×；ktest 单核串行窗的绝对恒等口径在 SMP EL0
     不可移植——判例候选）。
  4. **sw 断言语义修正（单相绿全相红）**：首版断言 sw0/sw1 各自严格增；
     全相跑相 6 红 `CPU0 sw 零前进` 而单相手跑同断言绿。定谳：**per-CPU
     sw 零前进在「满载荷独占核」是物理正常态**——root 独占 CPU0 纯 EL0
     churn（svc 进出不计任务切换、rq 空无抢占对象→零切换）；单相绿是
     boot 沉降提前退出、demo 面恰在窗内活动的**碰巧**（全相跑 TCG 烤慢
     →沉降帽跑满 60s→demo 毕→CPU0 只剩 root 自旋）。修：断言改**合计式
     sw0+sw1 严格增**（调度器双核活性——master poll 节拍必涨其所在核）；
     双核各自跑载荷的结构证归 spread 路由（root 家核 + 写者 K=2→CPU1）
     + dones 全满，不靠 sw 面。
- 单相双绿：`ALLOC ok n=320`（2s）+ `HT ok n=320`（2s，静默点双走查
  Ok(0) 过——含风暴前/后 integrity_check + used_count==0）。
- 全相 smoke-smp 七相回归：首跑相 6 红（尸检④）→ sw 合计式修正后
  **七相 PASS**（boot=2s / rootfs=210s / LB c1=78 / ASIDR n=2 /
  TLBS n=10 / ALLOC n=320 / HT n=320）。
- **变异标定连环两株绿吞 → 尸检⑤⑥定谳（join 后断言恒真洞）**：
  - 株 HT-1（模板环尾 create 不 close）注入后静默点②不红——三轮探针
    定谳（used=0 但窗内 handle 值在 = create 真成功；执行性株 HT-1b
    out 指完成轮数槽 PANIC 17039370≠320 证段执行）→ **真凶 =
    `sys_thread_exit` 末线程钩子触发 Process 收口 `proc.ht.drain()`
    （syscall.rs:1132）**：join 后表必被 drain 清空，静默点②
    used_count==0/integrity_check==Ok(0) **恒真断言无活性**。
  - 株 ALLOC-1（churn 换单 VMO 持续 commit 吃帧）注入后水位带宽不红
    ——同因：worker thread_exit → drain 子表 → 泄漏 VMO 全关帧全还，
    终账（join 后）被退出语义洗成恒过。
  - **修法 = 两相全改二次放行形**（S-5b 断言活性设计修正）：模板 3: 段
    = 就绪位自宣（win+0x98——0x94 踩 0x90 槽高 4B 曾炸 +2^32，尸档
    21474836800=5×2³²+320）+ park 自旋第二放行位（flag 页+8）→ master
    就绪收割（storm_poll 参数化 done_off，满值随形态 DONE=320/READY=5）
    → **真静默窗终账**（水位带宽/sw 合计/走查——帧与 handle 全在册、
    drain 未发生）→ 二次放行全员退（drain 归还属进程生命周期正确语义，
    非断言面）→ Σ DONE 槽终账 n=320。
- **变异标定两株（二次放行形后，双双活红）**：
  - 株 HT-1：`PANIC @ 静默窗走查 Ok(5) ≠ Ok(0)`——5 残留 handle 被
    integrity_check 抓获（走查断言先炸，used 断言备用）。还原 → 绿。
  - 株 ALLOC-1：`PANIC @ 水位越带 base 812 → end 1127 Δ315`（预期
    +320，实差 315 = 量级全合）。还原 → 绿（残留 grep x24/x25/MUT
    零 + 双单相复绿）。
- test-host 78 套件绿零挂；净态双单相 `ALLOC ok n=320` + `HT ok n=320`。
- 全相 smoke-smp 七相终验（二次放行形）：**PASS**（SMOKE_SMP_EXIT=0——
  boot=5s / rootfs=204s / LB c0=0 c1=78 / ASIDR n=2 / TLBS n=10 /
  ALLOC n=320 / HT n=320；尸档 /tmp/s5b-smoke-smp-final4.log）。让场三跑
  实录：两跑被用户设备壳 VNC rig 长占拦（协议干净拒跑零损失）+ 一跑在
  点火后 75s 窗内被无通告短突发拦——事后定谳同源（用户设备壳 P0/P1
  迭代 boot，9cdb925 落库对上）；改门控链式重试（120s 全信号稳定净场门
  qemu∪18080-18083∪smoke-image + FIELD_NEVER_STABLE 帽 20min）一次过。
