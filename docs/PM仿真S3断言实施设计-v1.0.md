# PM 仿真 S3 断言实施设计（v1.0）

> 版本：v1.0（2026-11-07）。状态：实施刀（本刀 = PM-16 §4.4 步3「PSCI 探测
> + 挂起降级」；裸相 boot 快验过，权威腿 full smoke/负例回填）。
> 输入：`research/PM-16-sim设备族与九断言挂G0G5-v1.1.md` §4.4 步3、
> `docs/PM仿真S2断言实施设计-v1.0.md`（A2/A4/A8 先例）、ADR #67（挂起期
> 时钟语义——slept_ns/suspend_count 记账挂本表回填后裁）。
> 目标：PSCI 深睡能力探测三档现实（TCG 实测 / KVM spike / 真机 v2）落为
> boot 期能力行；TCG 路深睡请求降级为 freeze 原语循环并给出真冻结证据链
> 标记。零 ABI 面（不动 abi.toml/gen_abi.py）、零新 syscall 号。

## 1. 裁决（主会话三面侦察后拍板，本刀严格照此实施）

1. **探测载体 = arch 层新模块 `psci.rs`**（不扩 smp.rs、不抽共享 HVC
   底座）：模块内本地 `hvc_call`（asm 钉板照 `smp.rs` CPU_ON HVC 形——
   x0 inout 装 fnid，x4-x17 + x18 全 clobber，SMCCC 符号语义返 i64）。
2. **分支门照抄 smp_bringup 盒子**：`efi_boot || boot_el() != 1` →
   **不发 HVC**，一行降级 `PSCI susp=0`（后缀原因 `(EFI handoff)`/
   `(EL2)`——子串可 grep）。EL1 路才发 PSCI_VERSION。
3. **探测序**：PSCI_VERSION(0x84000000) 有效（≥0x00010000 且非 -1）→
   打诊断行 `PSCI version 0x…`（**非标记**，hex 大写——conduit 活的正向
   对照）；无效 → `PSCI susp=0`。有效时再
   PSCI_FEATURES(0x8400000A, x1=0x8400000C SYSTEM_SUSPEND)：返 0 →
   `PSCI susp=1`；否则 → `PSCI susp=0`。结果缓存
   `PSCI_SUSPEND_SUPPORTED: AtomicBool` + `suspend_supported()` 读面。
4. **probe 调用点**：`main.rs` `efi_boot` 钉存后、**ktest::run 之前**
   （PWRDEG 读缓存；EFI/EL2/TCG/KVM 四路同点打印）。
5. **降级执行体 = `syscall::request_deep_sleep(pinned) -> i64`**
   （ktest 直调，零 syscall 号）：supported 真值臂走完整 SYSTEM_SUSPEND
   HVC（fnid 0x8400000C，无参，x0 inout）——**返回即失败**（SMCCC 成功
   = 下电不归）落降级兜底；降级分支原语级复用 freeze 机器（precheck +
   freeze + 成功尾 freeze_ok 既有写点照走 + `note_degrade_freeze` 记账
   + token 立即 Drop 语义 auto_thaw/thaw——零持窗）。**打印零**
   （PWRKEY 先例：标记归 ktest 断言层读台账）。
6. **ktest 载体**：`test_freeze_cycles` 内联手搭面（fc_dummy/mk）提为
   模块级共用 `mk_freeze_task`（行为零变）；新测 `test_deep_sleep_degrade`
   = run() **末位新号**（ktest done 前，既有序不动）——三计数恰 +1 +
   canary 已 thaw 的真冻结证据链 + `PWRDEG ok d=1`（d 读 telemetry 台账）。

## 2. 三档现实表（SYSTEM_SUSPEND 探测判据——初版）

| 档 | SYSTEM_SUSPEND | 判据 | 状态 |
|---|---|---|---|
| TCG virt（-kernel，boot EL1） | 不可达 | PSCI_VERSION 有效（本机 QEMU 实现至 0x2 族）但 PSCI_FEATURES(SYSTEM_SUSPEND) 返 -1 → 探测行 `PSCI susp=0`；深睡请求恒走降级 | ✅ 本刀实测（`PSCI version 0x…` + `PSCI susp=0` 双行在席——实际值见 §5） |
| Linux KVM | 待验 | 预期 FEATURES 返 0 → `PSCI susp=1`；真 HVC 面 + slept_ns/suspend_count 记账断言 | ⏳ 独立 KVM spike（**不在 PWRDEG ktest 里调真 HVC**——x1=0 无效 re-entry，行为不可假设（受理即可能关机）；主 smoke 先因 `PSCI susp=1` 与 TCG 标记不符而红，分档标记随后裁） |
| macOS HVF | 未知 | PSCI_FEATURES/SYSTEM_SUSPEND 实装面未取证 | ⏳ 单独 spike；不得把 Linux KVM 预期外推 |
| 真机 | 待硬件 | ATF/SCP 固件 PSCI 实装，conduit/版本以板级为准 | ⏳ v2（硬件窗口） |

## 3. 形状

### arch 面（`arch/aarch64/src/psci.rs`，新文件）

- `hvc_call(fnid, x1) -> i64`：模块本地，unsafe 只进 asm 块；未传
  `preserves_flags`（Rust asm 默认允许破坏 NZCV，覆盖 SMCCC 语义；AArch64
  无独立 flags operand）；PSCI_VERSION 先截 u32 再判，
  `0xFFFFFFFF` 一律 NOT_SUPPORTED（不赌符号/零扩展）；
- `probe(efi_boot)`：门住路零 HVC 一行降级；EL1 路 VERSION→FEATURES 两段
  探测 + `PSCI_SUSPEND_SUPPORTED` Relaxed 存；
- `suspend_supported() -> bool` 读面；`system_suspend_hvc() -> i64`
  SYSTEM_SUSPEND 真值臂入口（TCG 结构性不可达——supported=false 门住）；
- 打印走与相邻 boot 行同款 UART 路径（`BerryOS boot` 同期窗口）。

### 内核面

- `main.rs`：`efi_boot` 钉存后插 `berry_hal_aarch64::psci::probe(efi_boot)`
  （ktest 前）；
- `syscall.rs`：`sys_process_suspend` 邻位 `pub(crate) fn
  request_deep_sleep(pinned: usize) -> i64`——失败路径照
  sys_process_suspend 错误返回形（precheck 不过 -EBUSY / 无匹配任务
  -EBADSTATE）；**ADR #67：slept_ns/suspend_count 记账待 KVM spike
  回填三档表后裁，本刀只留钩**；
- `telemetry.rs`：freeze 族邻位 `DEGRADE_FREEZE: AtomicU64` +
  `note_degrade_freeze()`/`degrade_freeze_total()`（进现有环不建第二
  数据库 #45）；
- `ktest.rs`：`mk_freeze_task` 共用钉板 + `test_deep_sleep_degrade`：
  基线读 degrade/freeze_ok/auto_thaw 三台账 → 直调 → 断言三计数**恰
  +1**（真冻结证据链——隔离「只 bump 计数器没真冻结」假绿）+ canary
  已 thaw → `PWRDEG ok d=1`（d = degrade_freeze_total() 台账读值）。

## 4. 标记与相构成

- 裸相：30 → 32，新增（markers.sh MARKERS + smoke-image.sh
  KERNEL_MARKERS 硬拷贝逐字同步——EFI 路探测行带 `(EFI handoff)` 后缀
  子串可 grep、ktest 在 EFI 路照跑，两行都在）：

```text
PSCI susp=0
PWRDEG ok d=1
```

- rootfs 51 / RX_MARKERS 2 不动；RX 基础合相 81 → 83（boot 32 + rootfs
  51）；smoke-image 总断言 79 → 81（EFI 十段 + 接续 3 + 裸相 32 + 生态
  36）。计数注释同步：smoke.sh / smoke-smp.sh / smoke-dpgsoak.sh /
  justfile / 规则 07 / 规则 12 / 雷册 §C 两快照（顺手矫正规则 07 rootfs
  相行「50」→51 的 S2 漏同步遗留）。
- `PSCI version 0x…` 是**非标记**诊断行（正向对照，不进任何断言面——
  TCG/KVM 版本数值不锁）。

## 5. 验证与负例

正例（裸相 boot 快验——实施刀自验；**实际值由主会话权威腿回填**）：

```text
PSCI version 0x00010001（实施刀裸相 boot 实测：QEMU 11 TCG virt，
PSCI v1.1；权威腿 full smoke 复核）
PSCI susp=0
PWRDEG ok d=1
ktest done / ROOTFS ok / 零 PANIC（实施刀裸相快验已过——/tmp 单次
boot 日志双标记在席零 PANIC（不定行数——日志尾 scheduler canary 仍在追加，不锁快照值；权威腿 full smoke 八跑待跑））
```

负例反转红一次（权威腿执行，不入库变异）：

1. 探测株：把 FEATURES 判据 `== 0` 临时反为 `!= 0` → TCG 落 `PSCI
   susp=1` → 裸相 `PSCI susp=0` 与 `PWRDEG ok d=1` 双缺席（PWRDEG 见
   susp=1 显式 skip，防 SYSTEM_OFF 杀 VM——两标记独立执法）；
2. 降级株：注释 `note_degrade_freeze()` → PWRDEG d 恰 +1 断言红；
   `freeze_by_pinned` 换 0 返回 → 返 -EBADSTATE，rc==0 红；「返回 1 但
   不置 HELD_FROZEN」的纯计数谎言株由 FRZCYC 窗内冻结位断言传递兜底，
   PM-03 前 worth 将窗口探针下沉到本测；
3. 门株：`-machine virt` 加 `virtualization=on`（boot EL2）→ 探测行
   `PSCI susp=0 (EL2)`，标记子串仍命中（run-el2 相零扰动自证）。

## 6. 演进

- KVM spike：回填 §2 表 KVM 列 + ADR #67 slept_ns/suspend_count 记账
  裁决（硬件载体 always-on 域计数器 vs RTC 差补）+ 深睡断言进 KVM 相；
- 真机 v2：回填 §2 表真机列（板级 conduit/版本）；
- EL0 深睡入口（PM-03 挂起四原语刀）：`request_deep_sleep` 机制面已就
  位，但接入前必须补 KL 持有、自冻拒绝与活 SINGLE_THREAD token 交叉
  互斥门（当前 ktest 直调窗无并发，不能把“逐位同 Drop”外推到 EL0）；
- sim-S2 §6 演进行「PSCI 探测与深睡降级」已指向本文档（v1.0 落）。
