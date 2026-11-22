# shadow 侧 hazard-lite 验证环落地（调度终窗第一刀）

Status: implemented

## Problem

SMP-01 §5.1（[SMP-01-多核调度面全面审计-2026-11](../../../research/调度深度调研/SMP-01-多核调度面全面审计-2026-11.md) §五.1）发现的全体系唯一机制级真缺口：shadow 谱系（`sched/src/policy.rs`）换表无 hazard 回收半边。旧形态 = 双槽 XOR 翻转（`SHD_WHERE.load() ^ 1` 选槽、Acquire 读即解引用）：写者 `bind_gen_shadow` 背靠背两次换表时，第二写固定覆写读者（`shadow_reconcile`——apply 热路径每调一次；`shadow_gen`）刚经旧槽号进入的槽——撕裂窗（半新半旧表）。双核终验 60 ok 未触发 = 终验载荷无背靠背 SHADOW_COMMIT 序列（01 号推断），即现有绿灯对缺口零覆盖；06 号 §4 第一刀定谳：CP-04 金丝雀若不先补环，测的是缺口不是协议。

## Decision

照 `active_snap`（policy.rs，active 侧现役验证环先例）同款机制闭合，协议判定律单源下沉 `berry-polface`（W3-Q20 face-crate 先例族——sched 依赖 HAL global_asm 只 aarch64 可编，host 测试面唯一通路是抽 face）：

- **`HazardGate<SLOTS, CPUS>`**（[polface/src/lib.rs](../../../polface/src/lib.rs) 新增节）：纯原子协调位（活跃槽号 + 每核 hazard 位），零 unsafe；`reader_enter` = 验证环（load(Acquire) → hazard store(Release) → fence(Acquire) → 复读验证未翻转，翻了换槽号重发）、`pick_write` = 避让扫描（跳过活跃 ∪ hazard，满覆盖 panic canary——「wedge 优于静默跨核腐坏」同 active 侧 expect 执法）、`publish` = fence(Release) + 翻转。
- **policy.rs 消费侧**：`SHD` 双槽→三槽（同 SNAPS 几何，「活跃 1 + hazard 收敛 ≤2」恒有空闲）；`SHD_WHERE: AtomicU8` 删，替 `static SHD_GATE: HazardGate<3, HAZARD_CPUS>`；`shadow_snap` 走验证环；`bind_gen_shadow` 走避让扫描+publish（仍持 POLICY_LOCK——③不变）。promote/demote/对账计数零改动（promote 经 shadow_snap 的验证环在锁窗内首验即过）。

host 竞争钉板（polface 测试面，justfile test-host 已挂）：

- V6-c 正向：两写者（Mutex 串行 = POLICY_LOCK 同形）× 3000 轮背靠背双换 + 两读者全程紧循环；断言零撕裂（128×u64 全行同 tag）+ tag 单调不回退。实测 63.5 万读 / 1.2 万次换表穿越，零撕裂零回退。
- V6-d 负对照：同载荷复刻修复前双槽 XOR 协议，**必须**捕到撕裂（实测 112 次）——证明 V6-c 的绿是协议承压不是载荷不足。
- V6-a/V6-b：确定性律（避让含陈旧 hazard 位、cpu 钳位）+ 满覆盖 canary。

## Alternatives considered

- **读者出窗清 hazard 位 + 写者自旋等**：可去 canary 但引入写者自旋（持 POLICY_LOCK 在 SVC 上下文等他核 IRQ 读者——单核屏蔽窗形态下写者等不到即死锁面），且背离 active_snap 模板（任务书明令照抄先例）。弃。
- **sched/host/ + rustc --test 单文件直编**（② 号刀同日同约束的选择）：可保协议留在 sched,但 HazardGate 是纯协调逻辑无 HAL 依赖,face-crate 是既有成例（polface 章程「抽离即唯一事实源」）且 ktest 消费路径不变。两路皆正,取 face。
- **sched crate 整体 host 可编**：HAL 的 Mach-O 段名/x18 保留寄存器硬阻（实测),cfg 手术面大。弃。

## Consequences

- 撕裂窗机制级关闭:写者背靠背换表不再覆写他核在窗读者的槽;「见新槽号必见新槽内容」(①)与「在窗槽不被复用」(②)两半边在 shadow 侧齐了。
- **canary 语义实测澄清**(重要):纯速度竞争(host 双写者紧循环、读者迭代慢于写者世代)下三槽满覆盖**可达**——hazard 陈旧跨两世代即触发 panic。内核部署形态(写者稀有 SVC 事件、读窗短)远离该域,但 CP-04 换表金丝雀设计时须知:满覆盖响 ≠ 代码 bug,是协议前置「读窗串行形态两核 hazard 收敛」被违反的执法。测试侧解法=轮制新鲜度(每轮放行前等两读者 hazard 收敛到当前活跃槽,构造性保证背靠背两写恒有空闲槽)。
- shadow 静态区 +4112B(SHD 双槽→三槽,.bss 零填充)。
- SHADOW_COMMIT op 接入(未来 v3)背靠背序列现在有协议承压面;SMP-01 §5.1 行动项闭环。

## 验证实录(2026-11-15)

- host:polface 18 测全绿(14 存量零漂移 + 4 新);V6-c/V6-d 各 10 连跑稳定;正向 63.5 万读/1.2 万换表穿越零撕裂零回退,负对照同载荷捕 112 撕裂。
- QEMU 裸相(kernel ktest 相):**32/32 标记三连绿**——①共享树(含本刀+并行②号刀) ②HEAD 干净 worktree ③HEAD+仅本刀两文件 worktree;POLICY ok/ktest done/SHD-K 链/全部调度 ktest(BENCH/BENCHWAKE/FRZCYC/MANY/PI)到齐。
- rootfs 相与代码无关的实证:HEAD+仅本刀 worktree 亦死于 `echod 18080: Errno 48 Address already in use`(多会话并发 smoke 抢主机口;当日 /tmp 里另有 berry-cli-verify 等三个会话 worktree 在跑 QEMU);共享树那轮缺失标记(从 FAT ok 起)全是 bsh 自测链(三号会话在途 bsh)+同款端口互抢签名(evsclient 超时)。本刀在 rootfs 相的功能面(shadow_reconcile 每 apply 三条指令)在裸相重载下已全覆盖。

## Related

- [SMP-01-多核调度面全面审计-2026-11](../../../research/调度深度调研/SMP-01-多核调度面全面审计-2026-11.md)(§五.1 缺口发现)
- [06-综合定谳-v1.0](../../../research/调度深度调研/06-综合定谳-v1.0.md)(§4 第一刀)
- 消费面:[sched/src/policy.rs](../../../sched/src/policy.rs)(shadow 谱系节)
- 并行同窗:② 号刀(忙核 arm-to-nearest-deadline)同批进 sched/arch/justfile——两刀文件集零交集
