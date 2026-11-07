# PM 仿真 S2 断言实施设计（v1.0）

> 版本：v1.0（2026-11-07）。状态：A2/A4/A8 已落地（build/test-host/full smoke + 负例反转红）。
> 输入：`research/PM-16-sim设备族与九断言挂G0G5-v1.1.md` §4.2/§4.4、
> `research/PM-17-三期综合与落地刀序-v1.1.md` §3.2、
> `docs/sim设备通道与契约设计-v1.1.md`（sim-S1 M1–M3 已收官）。
> 目标：把第一批三断言 A2/A4/A8 变成主 smoke 的状态谓词；不做功耗绝对值
> 断言，不引入新 syscall，不在 EFI/AAVMF 相挂 GPIO3。

## 1. A2：两轮冻结循环

### 裁决

PM-16 原案写「挂 ktest」。实施期事实修正：`ktest` 在
`berry_sched::init()` 之前运行，全局 SLEEPQ/RQ 尚不可承载真实受害者；
从 ktest 走 `NR_PROCESS_SUSPEND` 成功路径会因匹配任务数为 0 返回
`-EBADSTATE`。因此 A2 载体改为既有 `thrchild` 用户态全链自验：

- 真 syscall `berry_process_suspend(SINGLE_THREAD)`；
- 真调度队列冻结/解冻；
- 真 `SuspendToken` close/Drop；
- rootfs smoke 金丝雀天然在后续标记链上。

这不降低覆盖，反而覆盖更强：ktest 只能做对象级模拟，thrchild 走完整
EL0 syscall +调度+对象生命周期链。

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
```

`FRZ ok n=2` 进入 rootfs 51 标记；EFI/ECO 面随 THR 照打但不入 ECO_MARKERS 断言。

## 2. A4：无主唤醒正向对照

刀 1 已完成稳态负例：

- `WakeRec.source` 32B 内布局；
- `on_wake(NONE/UNKNOWN)` 写点 panic；
- `wake_check()` 拒绝非法 source；
- G1.5 断言 UNKNOWN 槽 wakes=0；
- WAKESRC 行含 `X=wakes/irq`。

S2 只补正向对照：QEMU `-kernel` 路 QMP `system_powerdown` 会脉冲
PL061 GPIO3，GIC INTID39。本仓不注册 GPIO 消费者，该 IRQ 进 trap 未知臂，
只计 UNKNOWN 到达，不 wake_matching。

### 内核面

- `IRQ_PL061_GPIO = 39` 入 `SPI_ROUTES`；
- 触发配置为 edge：100ms 电源键脉冲只产生一次到达；
- QEMU 模型契约：PL061 `istate` 锁存不随 IMSC 屏蔽；不写 `GPIOIC` 时
  MIS 恒高，而 GIC edge 只认 MIS 0→1 跳变，因此每 boot 恰一次到达，
  宿主重复 `system_powerdown` 是 no-op。A3 若引入清锁，须重审该契约；
- 仅在 `opt/berry/simdev` 门内：
  1. `enable_spi_routed(39)`（会清历史 pending）；
  2. 写 PL061 `GPIOIE` bit3（v1 全写有意：寄存器复位 0 且 bit3 唯一
     消费者；IS/IBE/IEV 用复位默认）。
- 次序不可反：若先开 GPIOIE 再 GIC enable，唯一边沿会被 `ICPENDR` 清掉。

trap 未知臂已有 `UNKNOWN_SEEN`，G1.5 `wake_src_merge_hal()` 会并入
WAKESRC `X=0/N`。无 handler、无等待者是本断言的目的形态。

### smoke 面

RX 相启动后立即后台循环 QMP `system_powerdown`，直到串口出现：

```text
WAKESRC acct=... X=0/[1-9][0-9]?
```

行尾上界 ≤99：下界证明无主 IRQ 确实到达，上界拦“误配电平/清锁重投”
风暴。注入器预算 = RX 相 `DEADLINE_ROOTFS + 10s`（慢 TCG 不先于相门
自死）；终判先看串口形状，再回收注入器。随后 marker sweep 继续要求 RX
交互、INPUTP、FAT 等后续链到齐，证明无主 IRQ 后系统存活。`irq? id=39`
只是辅助诊断，不作硬门（print preempt 窗可能抑制）。

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

- rootfs：50 → 51，新增：

```text
FRZ ok n=2
```

- RX 基础合相：79 → 80（boot29 + rootfs51），另有 RX 专属 SIMBAT。
- A4 不新增标记：复用 `WAKESRC acct=`，并加形状断言

```text
X=0/[1-9][0-9]*
```

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
2. A4：把 WAKESRC 形状断言改为 `X=0/0` 或不使能 SPI39，期望 smoke FAIL；
   风暴株把 SPI39 配置临时改成 level，期望行尾上界 `≤99` 拦截 UNKNOWN
   重投膨胀；
3. A8：把某个 ST_BAD 期望改为 ST_OK，期望 `siminj --profile a8` rc=1。

以上均为一次性验证，不入库变异。

## 6. 演进

- A3 唤醒风暴合并率：等 timer slack 刀；PL061 `GPIOIC` 清锁也在 A3。
- lease/power·mode 真 `-EINVAL` 面：随 berry-power 骨架刀。
- PSCI 探测与深睡降级：sim-S3。
