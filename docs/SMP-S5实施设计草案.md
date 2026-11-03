# SMP-S5 实施设计草案（G2 关卡收官——并发压测集 + 双核 soak 刀）

> 母案：`docs/SMP设计草案.md` §4 S-5 行 + §5 验收面。
> 出口：`docs/测试策略.md` G2 行「待办」→「已过」+ G2 话术落 AGENTS（§7 四条限定对表）。
> 前置：S-4 全毕（五相 smoke-smp PASS + TLBS n=10 + 04feec2/2aaf8c2 落库）。

## §0 replan 记录（LoopX periodic review 2026-11-03）

- 后继 todo：`todo_8fe4a3028e11dd7aa05bc04f`（[P1] S-5 G2 关卡收官实施刀）。
- 六路侦察 workflow（lanes 1-6）+ 批评员十漏面 → 四大裁决 + 刀程六段（§2/§3）。
- vision 面继承 E1 收官 replan：S-0..S-4 毕，SMP 线唯一余刀 = 本刀；E2 逐域下放不占刀。

## §1 现状侦察结论（2026-11-03，基线 2aaf8c2）

### 1.1 G2 出口规格（冻结）

- `测试策略.md` G2 行：`| G2 双核 SMP | 负载均衡对、TLB shootdown 对、无竞态 | -smp 2 + 并发压测 | 待办 |`
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

## §9.1 S-5a 执行追记（2026-11-03 进行中）

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
