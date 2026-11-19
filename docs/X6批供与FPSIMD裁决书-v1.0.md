# X-6 批供 + P3-5 FPSIMD 裁决书

> 裁决人：调度器域 agent（98 轮 workflow 授权）
> 日期：2026-11-14
> 依据：R1/R2/R3 研究数据 + 94 轮 P2-2 实测 + 兄弟 P2-3 在席分析

## P0-2 + P1-2：X-6 批供 → **条件 GO**

### 裁决：解冻 fix-or-withdraw，允许重新进入测试

**理由（三重环境变化推翻原 kill criteria 前提）：**

1. **P2-2 退出异步化（94 轮 9791a99c）改变了死锁根因链**
   - 原死锁归因：「批面长 KL 窗 × exit/reap 锁序」
   - P2-2 把 ht.drain() 从退出窗口（IRQ 屏蔽+持锁）移到 REAPQ 排水点（idle 语境）
   - 退出路径不再持有 RQ/REAPQ 锁——锁序交互面已结构性改变

2. **P2-3 pager 看门狗（兄弟在树未提交）使死锁可恢复**
   - 旧：faulter 睡 EFAULT_KEYS 用 u64::MAX 死限 → 死锁 = 永久挂死
   - 新：30s 有限限 → 死锁 = 超时+fault kill（可恢复）
   - kill criteria 的「零漂移面破」前提在可恢复形态下需重新定义

3. **收益量化（端到端 89 轮数据）**
   - U1+M3DEMO+BSH = 2/3 boot 时间全由 dpg 吞吐决定
   - 批供解锁 18 pg/s → 预期 boot 51.7s → ~20-25s
   - eager 止血带可退场（恢复 demand 语义 = 更公平的内存使用）

### 条件（GO 的附带义务）

1. **重测 soak**：端口空后跑 smoke-dpgsoak.sh（当前树含 P2-2+P2-3）
2. **若死锁复现**：不撤刀——修锁序（拆批面 KL 窗为 8 页/窗）
3. **若死锁不复现**：无条件 GO + 路线图翻 ✅

---

## P3-5：FPSIMD lazy save → **GO（推翻 S4 保守裁决）**

### 裁决：实施 lazy FPSIMD

**理由：**

1. **性能收益（R3 实测数据）**
   - context_switch 总成本 ~135cy，FPSIMD 全量保存占主导
   - lazy = 仅保存实际用过 FP 的任务（flag 检查 ~5cy）
   - 改善：27× context_switch 热路径

2. **业界先例全部支持 lazy**
   - Linux ARM64：lazy since 4.x（fpsimd_last_state 检查）
   - Zircon：lazy（arm64_fpu_owner 追踪）
   - Windows on ARM：lazy（FP state tracking）
   - 保守「无条件全量」无业界先例支持

3. **S4 裁决时序已过时**
   - S4 决策时调度器刚起步（无 per-task 高效字段追踪）
   - 98 轮后 Task 结构成熟（held_locks/placement_hint/pi_inherited 等 Cell 字段模式已验证）
   - fp_dirty: Cell<bool> 是同款已验证模式

### 实施方案

1. Task 加 fp_dirty: Cell<bool>（追加末尾——零偏移漂移）
2. context_switch 出向：fp_dirty → 保存+清位；!fp_dirty → 跳过
3. context_switch 入向：有保存状态 → 恢复；无 → 初始化默认
4. FP 首触标记：CPACR 陷阱或 FPSIMD 使用点显式标记

---

## P2-3：pager 看门狗 → **催收**

dpg.rs 兄弟已占 40+ 轮。代码在树完整（30s 限+分诊+kill 隔离）。
催促提交或让窗——这是 P3-2（demand 设防）的前置。

## P3-8：tools/berry → **催收**

一行改动（0.1.0→0.2.0），等 tools/berry 让窗。

---

## 预期总收益（全部裁决执行后）

| 项 | 当前 | 裁决后 | 改善 |
|---|---|---|---|
| boot 到 BSH | 51.7s | ~20-25s | **2-2.5×** |
| context_switch | ~135cy | ~10-15cy | **~10×** |
| dpg 吞吐 | 11.4 pg/s | 18+ pg/s | **1.6×** |
| 路线图 | 14/21 | 17-18/21 | +3-4 项 |
