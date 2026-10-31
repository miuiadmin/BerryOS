# 08-QEMU与仿真-QEMU 与仿真差异

> 一句话定位：动 QEMU 机型/引导参数/时率解读 前先读本文。判例全文 → docs/雷册-判例库.md（瘦行行尾锚点直达）；设计文档见文末「深读」。

## 域铁律

### ARM64 目标约定

- 默认目标 `aarch64-unknown-none`（内核 + 用户态 no_std）；如需特殊段布局用自定义 `target.json` + linker script。
- 内核运行在 EL1，用户态 EL0；**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`virtualization=on` 则 EL2），别假设固定 EL——EL 检测/降级与异常级别切换集中在 `arch/aarch64/`（机制细节 → 雷册 §A5）。
- 调试与验证主用 **QEMU `qemu-system-aarch64`**（virt 机型）。

### QEMU 运行时行为

- **QEMU ≥11.x 不接受 `-bios none`**（报 `Could not find ROM image 'none'`）；`virt -kernel <ELF>` 直加载即可，故 justfile / `.cargo/config.toml` 都不带 `-bios`。
- **QEMU halt 后不退出**：内核 `wfe` 死循环，串口打印完即停在那。退出 QEMU 用 `Ctrl-A` 然后 `X`（macOS 无 GNU `timeout`，自动化里用后台 + `kill`）。
- 调试：`-s -S` + lldb 单步；`-d cpu_reset -D qemu.log` 看启动 CurrentEL。

> 口径保活：§A8 瘦行的「tick 率 ≈ SLICE_HZ/2」探针串为冻结口径——`tools/smoke.sh` 金丝雀时限注释依赖此串，重排本文时一字不改。

## 判例瘦行

- **SMP/TCG 运行时三则（S-1）**：①PSCI conduit 按机型分型：HVC→PSCI 拦截只在默认 `-machine virt`（boot EL1）成立——virtualization=on（boot EL2）降 EL1 后 `hvc #0` 陷阱到 EL2 → VBAR_EL2=0 → PC=0 挂死；修法 = 入口 EL 门（`_start` 降级前读 CurrentEL 经 x20 携带、PA 写 `BOOT_EL`）；②TCG `wfe` = 立即返回 hint（忙转 ≈ 拖主核 ~40%）、`wfi` 才真睡——短窗握手 wfe 无妨（毫秒级 + sev 唤醒），长驻/稳态 park 一律 wfi；③双核 MTTCG 吞吐 ≈ 单核 0.61x——`-smp 2` 线一切时限帽 ×2 起步（反向 = 假超时）。先问：conduit 分型过吗？这个等是握手还是驻留？时限帽按核数缩放了吗？ → 雷册 §A98/99/101
- **QEMU aarch64 `virt` 起始 EL 随配置而变**：默认 `-machine virt`+`-kernel` ELF 起始 EL1；`virtualization=on` 起始 EL2（走 EL2→EL1 eret）。boot 第一行 `global_asm!` `mrs CurrentEL` 检测并按需降 EL1（关 MMU/cache、写 `sctlr`、`eret`）——**别假设固定 EL**。先问：当前机型配置下起始 EL 是哪个？这段 boot 代码假设的起始 EL 在两种机型下都成立吗？ → 雷册 §A5/§B8
- **PAC/BTI/MTE 目标 day-one 开，但 Phase 0/1 推迟**：`cortex-a72`（ARMv8.0）无此扩展；v2 切 `-cpu max` 时三档分刀（SEC-01 C4）：BTI≈零改动 / PAC=key 管理小刀 / MTE=分配器中心点刀（需 `-machine virt,mte=on` + slab/buddy 改造）——旧「源码零改动」仅对 BTI 严格成立（见 `安全与硬件隔离设计.md` §3/§6）。先问：这个安全扩展在当前 `-cpu` 下存在吗？ → 雷册 §A7
- **QEMU TCG 下 CNTV 实测 tick 率 ≈ SLICE_HZ/2**：`CNTFRQ_EL0`=62.5MHz、SLICE=`FREQ/SLICE_HZ`=6.25M 配置正确（`SLICE_HZ=10` 应得 10Hz）但实测抢占 ~5Hz——TCG `virt`/`cortex-a72` 下 `CNTVCT` 推进速率 ≈ `CNTFRQ/2`。**非内核 bug**：真机或 `-icount` 应为 10Hz。先问：tick 率减半是内核 bug 还是 TCG 2x 效应？ → 雷册 §A8
- **QEMU 由 SIGTERM 终止后仍存活数秒并继续刷盘（P6 A-6）**：盘取证/复用已写盘前必须等进程真退（kill 后 wait 循环 + 再 sleep 3）；smoke 相位切换用 `kill` + `wait` 串行化，全 `-drive` 线 `cache.writeback=off`（writeback 模式 kill 可丢页缓存——缓存落宿主盘的保证随 off 才成立）。先问：取盘/复用前等 QEMU 真退了吗？ → 雷册 §A78

## 深读

- docs/开发体验.md —— 四级阶梯 + 日常循环：QEMU 日常命令与调试手法（`-s -S` + lldb、日志留取）的展开面。
