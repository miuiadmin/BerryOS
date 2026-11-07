# PM 仿真 S2 断言实施设计（v1.0）

> 版本：v1.0（2026-11-07）。状态：A2/A4/A8 已落地（build/test-host/full smoke + 负例反转红）。
> 输入：`research/PM-16-sim设备族与九断言挂G0G5-v1.1.md` §4.2/§4.4、
> `research/PM-17-三期综合与落地刀序-v1.1.md` §3.2、
> `docs/sim设备通道与契约设计-v1.1.md`（sim-S1 M1–M3 已收官）。
> 目标：把第一批三断言 A2/A4/A8 变成主 smoke 的状态谓词；不做功耗绝对值
> 断言，不引入新 syscall，不在 EFI/AAVMF 相挂 GPIO3。

## 1. A2：两轮冻结循环

### 裁决

PM-16 原案写「挂 ktest」。实施期事实修正：`ktest` 主序列在
`berry_sched::init()` 之前运行，全局 SLEEPQ/RQ 尚不可承载真实受害者；
从 ktest 走 `NR_PROCESS_SUSPEND` 成功路径会因匹配任务数为 0 返回
`-EBADSTATE`。因此 EL0 载体改为既有 `thrchild` 用户态全链自验：

- 真 syscall `berry_process_suspend(SINGLE_THREAD)`；
- 真调度队列冻结/解冻；
- 真 `SuspendToken` close/Drop；
- rootfs smoke 金丝雀天然在后续标记链上。

**终裁：双载体**。thrchild 之外，ktest 尾段（SUS-NEG 之后、BL-OVF 之前
——彼时 sched/对象面已就位）复活 EL1 对象级 svc 面 `test_freeze_cycles`：
手工构 `Task`/`Process` 钉 sleepq 后直调 svc4（`NR_PROCESS_SUSPEND`），
绕开「init 前不可承载真受害者」的窗口限制。EL0 真消费者链（thrchild）+
EL1 对象级 svc 面（ktest）双执法：裸相 `FRZCYC ok r=2` / rootfs
`FRZ ok n=2`——ktest 面吃规格书原文（两轮 + 计数严格递增 + 金丝雀不熄），
thrchild 面吃真消费者链，覆盖互补不冗余。

### 形状

每轮：

1. t3 重新 `sched_setattr(FREEZE_READY)`；
2. t1 `process_suspend(t3, SINGLE_THREAD)` 得非零 token；
3. 冻结中：
   - `sched_getattr` 仍见 READY；
   - 共享闸门 `THR3_GO == 0`；
4. close token；
5. `sched_getattr` 验 READY 已自动清除。

执行两轮后才置 `THR3_GO=1`，t3 写哨兵并退出。任一轮失败均退码 9，
svcmgr 不打标记。

内核侧补齐 `telemetry::freeze_ok()` 接线：进程冻结与单线程冻结的
`insert_suspend_token` 成功尾均计数。既有 `tele:` dump 点在 boot 期
AIDEMO，先于 rootfs/thrchild，因此本刀的可观测验收不走该行，而走
thrchild 退出码 8 → `FRZ ok n=2`；`freeze=` 的用户 land 后打印点随后续
遥测导出刀消费。

### 验收

```text
THR ok
FRZ ok n=2
FRZCYC ok r=2
```

`FRZ ok n=2` 进 rootfs 51 标记（thrchild 轮数以退出码 8 带出、svcmgr
收码打标——防子进程标记字面量被冻结路径吞掉）；`FRZCYC ok r=2` 进裸相
30 标记（ktest test_freeze_cycles 尾段打：两轮 depth==1 + token 序贯
+ `freeze_ok` 严格递增恰 +2 + auto_thaw 恰 +2 + 金丝雀每轮冻结/解冻窗
held_locks 双断言）。EFI/ECO 面随 THR 照打但不入 ECO_MARKERS 断言。

## 2. A4：无主唤醒正向对照

刀 1 已完成稳态负例：

- `WakeRec.source` 32B 内布局；
- `on_wake(NONE/UNKNOWN)` 写点 panic；
- `wake_check()` 拒绝非法 source；
- G1.5 断言 UNKNOWN 槽 wakes=0；
- WAKESRC 行含 `X=wakes/irq`。

S2 补两件：**①稳态窗未知醒因=0**（绝对断言——任一未知 IRQ 号到达即红，
非仅窗口内增量；BLOCK_BOOT_DONE 后叠加 `wake_check()` ⑤ 环归因复检）；
**②无主到达正向对照**：QEMU `-kernel` 路 QMP `system_powerdown` 脉冲
PL061 GPIO3（SPI 7 = GIC INTID 39）。v1 不注册 GPIO 消费者，该 IRQ 获
专属源 `WAKE_SRC_POWER_KEY = 0x07`（**到达-only**：IRQ 级记账 +
no_waiter 台账，恒无唤醒、永不进环——与 NONE/UNKNOWN 同类三号位）。

### 内核面

- `IRQ_PL061 = 39` 入 `SPI_ROUTES`，触发**电平** 0b01——DTB 实证
  `pl061@9030000 interrupts = <0 7 4>` = LEVEL_HIGH（GIC 面真值纪律）；
- 新 arch 模块 `pl061.rs`：handler = MIS 确认 → **ICR 写 1 清边沿锁存
  （EOIR 之前去活铁序——GIC 电平触发线挂 MIS!=0，不 ICR 则 EOIR 后
  立即重投活锁）** → 通知槽；检测型 IBE 双边沿 bit3（极性无关——单脉冲
  按下/抬起两沿各锁存一次，≤2 IRQ/脉冲，smoke 风暴上界按此校准）；
- 通知槽 `set_pl061_notify` 先注册后使能（槽空 = 首达静默无人记账）；
  内核侧 `pl061_notify` 双台账（`note_irq_arrival` + `note_irq_no_waiter`
  ——结构性无主，每次到达都记，区别于 uart1 的「n==0 才记」）+ 标记行
  `PWRKEY unowned n=` **读 telemetry no_waiter 台账**（telemetry 被拆/
  漏记时 n=0 直红；本地计数器会让变异假绿——审查裁决改源）+ print
  preempt 窗让位（QMP 重注入可补拍，台账不丢）；
- `WAKE_SRC_POWER_KEY=0x07` 入 abi.toml [[wakesrc]]（add-only；0x08–0x7E
  预留）：`on_wake(POWER_KEY)` 写点 panic（L0——无主 IRQ 长出唤醒路径
  即违约前置）、`wake_src_valid` 白名单有意不含、watchdog 稳态窗
  POWER_KEY 槽 wakes==0 终检 + WAKESRC 行 `P=w/i/nw` 段（boot 期快照
  面，注入前恒 0/0/0 =「boot 期无持有者」证据；注入期数值面由标记行
  承载，注入后读点挂 0x84 telemetry_open 落地刀）；
- 门 = fw_cfg `opt/berry/simdev`（与 UART1 同线；PL061 板级常在——门裁
  「是否使能」不裁「是否存在」），`pl061_init` 次序：ICR 清历史锁存 →
  脚配置 → GIC 使能 → IE 最后开。

### smoke 面

RX 相 SIMBAT 段后挂 **PM 微相**：`qmp_powerdown` 注入 → 轮询
`PWRKEY unowned n=` 出现（60s 帽 + ≤3 次重注入；QEMU-alive 门并行——
无主 IRQ 不许击杀内核，QEMU 退出即红）→ **值域门 `1 ≤ n ≤ 6`**：n=0 =
台账漏记红；n>6 = 3 注入 × 2 沿之上 = ICR 去活/风暴破绽红。该标记入
RX_MARKERS 第 2 元（markers.sh 单源，sweep 防死单源）。`irq? id=` 诊断
行不作硬门（print preempt 窗可能抑制）。

## 3. A8：电源类别毒环

### 载体裁决

`lease/power·mode` 服务尚未存在，不能为断言预铸无人消费的内核 op。
S2 采用 PM-16「pwrcls 式注入」的纪律面：以 sim-S1 已冻结的
`berry-pwproto` 帧为毒样本载体，验证非法电源类别：

- 不推进状态；
- 不腐蚀 last_seq 记账；
- 不产生 SIMBAT；
- 解码器可恢复；
- 后续合法帧照常 ACK/打标；
- 系统与 FAT 恢复链继续存活。

这保持「机制在内核、策略/治理在用户态」边界；真正 lease/power·mode 的
`-EINVAL` 面随 berry-power 骨架刀落地。

### 九向量

`tools/siminj.py --profile a8`：

1. 未 init + battery status=4 → `ST_BAD`；
2. 未 init + dev=thermal（合法 wire 设备、错服务类别）→ `ST_BAD`；
3. dev=9（未知类别）→ 无 ACK，解码器按杂音再同步；
4. seq1 合法 → `ST_OK` + `SIMBAT seq=1`；
5. seq2 合法 → `ST_OK` + `SIMBAT seq=2`；
6. 重放 seq1 → `ST_STALE`，无标记；
7. seq3 capacity=101 → `ST_BAD`，无标记；
8. seq4 health=8 → `ST_BAD`，无标记；
9. seq3 合法恢复 → `ST_OK` + `SIMBAT seq=3`。

硬断言：

```text
ALL-A8-PASS
SIMBAT ok seq=1
SIMBAT ok seq=2
SIMBAT ok seq=3
SIMBAT 总行数 == 3
```

### 挂点

B4 Boot C 在 FAT 毒盘恢复标记到齐后加 serial1 + `simdev` 门，注入 A8
九向量。随后仍要求 QEMU 存活、SIGTERM 后盘面 `pwrcls --classify` clean。
这同时覆盖：

- FAT 毒环恢复；
- 电源类别毒环；
- 毒后恢复；
- guest 标记 + 宿主盘面双不变量。

Boot A/B 不挂 sim 门，保持原掉电采样零扰动。

## 4. 标记与相构成

- 裸相：29 → 30，新增：

```text
FRZCYC ok r=2
```

- rootfs：50 → 51，新增：

```text
FRZ ok n=2
```

- RX 基础合相：80 → 81（boot30 + rootfs51）；RX 专属 SIMBAT/PWRKEY
  （RX_MARKERS 1 → 2）。
- A4 新增 RX 专属标记 `PWRKEY unowned n=`（值域门 1≤n≤6 + QEMU-alive
  + 重注入帽 3）。
- A8 不新增 guest 标记：非法样本静默是断言，恢复后恰三条 SIMBAT。
- smoke-image/EFI 不加 A2 FRZ 与 A4 GPIO 断言：ECO 面与 GPIO3 ACPI 分流
  不兼容。

## 5. 验证与负例

正例：

```text
just build
just test-host
just smoke
```

负例反转红一次：

1. A2：把循环上界 2 改 1，thrchild 退码 7，svcmgr 只打 THR、`FRZ ok n=2`
   缺席；去掉第二轮 re-READY 自证则第二轮 suspend 返回 -EBUSY、退码 9；
2. A4：不使能 SPI39（或拔 pl061 接线）→ `PWRKEY` 标记 60s 缺席红 +
   QEMU-alive 面自证；telemetry 漏记株（注释 `note_irq_no_waiter`）→
   n=0 值域下界红（标记行读台账，本地无计数器可假绿）；风暴株（临时
   拔 ICR 清锁写）→ n>6 上界红（重投活锁被值域门拦）；
3. A8：把某个 ST_BAD 期望改为 ST_OK，期望 `siminj --profile a8` rc=1。

以上均为一次性验证，不入库变异。

## 6. 演进

- A3 唤醒风暴合并率：等 timer slack 刀；PL061 `GPIOIC` 清锁也在 A3。
- lease/power·mode 真 `-EINVAL` 面：随 berry-power 骨架刀。
- PSCI 探测与深睡降级：sim-S3（已落 v1.0——`PM仿真S3断言实施设计-v1.0.md`）。
