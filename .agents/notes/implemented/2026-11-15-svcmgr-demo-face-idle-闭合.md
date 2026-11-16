# svcmgr idle 后半刀——demo-face 门递达+单核 idle 停泊补(06 号闭合)

Status: implemented

## Problem

06 号修窗件 1 的 kernel 半刀(fw_cfg `opt/berry/demo-face` 门 task_a/b+舰队+watchdog)落地后,OFF 态残余全在 svcmgr 域:talker 'd' 洪流+echo 'v' 环无条件常转(bare OFF 实测 49.2%,desktop OFF 99.9%——[前刀 Residual](2026-11-15-demo-face-fwcfg-gate.md) 定谳)。svcmgr 不读任何 fw_cfg/bootparams,门值无通道可达 EL0。

## Decision

三件同刀:

1. **通道 = spawn env gift(x1 第二参)**。裁决:句柄表新插槽位——裸相/rootfs 相/fb 相的相位序定值 1-9 全占(svcmgr 侧 114 处槽位常量消费),重排死刑;sysver VMO 扩字段——fb GOP 相位专属(裸相缺席)+编译期常量页像破「零运行时取数」设计;新 syscall——ABI 号位预算 42/45,单 bool 面过重。x1 = spawn 期注入、零新 ABI、全相位恒在。落面:arch/aarch64/src/task.rs(trampoline `mov x1,x22`+`new_user` arg1→r[3])+sched 三函数签名链(`try_prepare_user`/`try_spawn_user`/`new_user_with_as`)+`spawn_user_env` 新口(`spawn_user` 签名不动,9 处既有调用点零改);内核 main.rs svcmgr spawn 改 `spawn_user_env` 递 `USER_ENV_DEMO_OFF` 位(svcmgr 侧同值镜像,两侧对读维护)。
2. **svcmgr 门**:`_start(console, env)`;门关 = B 半(echo+roundtrip+`SVC ok`)/A 半(talker+echo2+交接)整体让位;稳态环 echo 往返+'v' 打印随 `end0==0` 跳过;门关+三路皆空(APP_SVC/launcher/子表)新增 10s 帽长睡停泊臂——**throttle() 是纯忙环节流器(3.1M 次 black_box 自旋)非时点**,门关态圈速兜底由停泊臂承接。PM 双服务预拉/guifbd/wmd/launcher/boot 自验全不动(真服务非演示)。
3. **单核 idle_loop 停泊补(实施中发现的结构缺口)**:旧判据「rq 空∧SLEEPQ 空」才 park——任何定时睡者(svcmgr pager 50ms/svcmgr 空闲帽 10s/simbattery 1h)在即永走 arm_slice+yield_now 热旋支(实测 PC 10/12 落 `ht_drain_flush`→kern_acquire 记账头,99.6% 旋、串口静默、switches 冻结——忙环时代 rq 恒非空直转+演示清空后旧代码退化空环 yield 双重掩盖)。06 号 §3-4 以为在位的 arm_at 停泊**只在 SMP/par_enabled 分支有**,单核分支缺。修 = SMP 分支(:3134-3155)同形下放:rq 空即停泊(SLEEPQ 最近 deadline `arm_at` oneshot/尽空 `mask`+wfi),rq 非空支 arm_slice+yield 逐字原样。

## Residual discovered

- **yield Σvruntime 账本灌水(停泊补唤醒的真 bug,已同刀修)**:单核 idle「有活」支从 IDLE 让出,而 `Fair::yield_slice_return` 的调用契约是「让出者在环」——IDLE 永不在环,无条件抬值 = ledger 独加、walk 不含环外者。停泊补使「醒→入环→yield from IDLE」每醒一灌,OFF 态实测 1.38M 漂移被 ring_check 百切换金丝雀响亮捕获(`KERNEL PANIC at runqueue.rs:234`)。修 = `yield_now_impl` 在环(`rq_prev.get().is_some()`——link 置 Some/unlink 清 None 的结构不变式)才抬。语义边界:任务侧 yield(0x3F 消费者)不受影响——任务让出时恒在环。
- **switches 金丝雀持续打印**:OFF 态稳态 ~47 切/s(pager 20Hz×2+svcmgr 0.1Hz)→ 每 ~2s 一行 `switches=N kl=0`(bring-up 诊断,预存行为非本刀面)——06 号 §4-5「idle 断言=串口静默+vCPU halted」口径下这是已知噪声源,归 06 号 §4-4 余量件(pager 事件化)同族后续。
- **HEAD 混合态(并行会话纠缠,如实)**:svcmgr 门(x1 消费面)被并行 GUI 授权会话随 5452649af 连带提交,内核侧通道(x1 递入面)在本刀未提交集——HEAD 单独构建时 svcmgr 读垃圾 x1(demo 门随机)。owner 落本刀 kernel/sched/arch 文件后自愈。

## Verification

Apple M3 Pro/QEMU 11.0.3 TCG,采样 = QMP query-cpus-fast thread-id 的 proc_pid_rusage 双点 60s 窗(06 号 §1.3 纪律):

- **bare OFF**(`-fw_cfg demo-face,string=off`):**0.22% user + 0.04% sys**(目标 <5%,06 号 ~0% 预期兑现);改前同树同窗 99.65%(旋在 reap_drain→kern_acquire);boot 串口 `DEMO gate off` 恰一次,`SVC ok`/`DEMO ok`/echo/talker/echo2 零行(AIDEMO ok 为子串假阳性),boot 自验标记(THR/FRZ/DPGP/DPGS/DPGN)全在。
- **ON 裸相零漂移**:改前(9035af061 worktree 干净基线)vs 改后串口对拍——**32/32 标记双齐**,diff 全为舰队字母交错/计数时序噪声(无结构性漂移);PERF policy 数值面漂移归并行 M4 带骨架会话(class/policy 未提交变更,非本刀文件)。smoke 裸相 32/32 到齐。
- **GUI 形 OFF**(rootfs 双卷+display 800x600@60+ramfb+盘+net+tablet+echod/evs/skt):串口 `DEMO gate off` 恰一次、`SVC ok`=0、talker `DEMO ok`=0、app 链(APP ok n=9/APPUI ok n=8)毕后转 switches 金丝雀低频巡航,零 panic——定性达标;**CPU 数字窗两次被宿主 OOM 击杀**(并行会话 berry CLI EFI 256M QEMU+多探针并发,内存压)——bare 形数字+同一停泊路径为结构性论据,宿主净窗补测随终验。
- **smoke rootfs 相红=他人面(判别闭合)**:u3 步 6 `wc spawn failed` 弃链在**基线内核(9035af061,非本刀)同签名复现**(同 rootfs.cpio)——rootfs.cpio 烘进了并行 bsh 会话的飞行中快照(报错文案与现树已不同步),非本刀 kernel/sched/svcmgr 改动;该相绿须待 bsh 会话收口后复跑。

## Alternatives considered

- **x0 打包(console|flags<<32)**:两文件最小但污染「入口 x0=console 发端」三处在册契约,拒。
- **bootparams etc/ 通道复用**:demo-face 是宿主运行期注入(镜像外),etc/boot-params.toml 是装机期烧录(镜像内)——时序不同源,拒。
- **svcmgr 经 mmio VMO 直读 fw_cfg**:裸相无 mmio VMO+内核 fw_cfg 解析面在 EL0 复制,拒。
- **pager 任务快递(x0 携门值写共享 .bss)**:svcmgr/pager 双任务就绪序无保证,等待面引死锁险,拒。

## Related

[demo-face fw_cfg 门(kernel 半刀)](2026-11-15-demo-face-fwcfg-gate.md)(前刀+Residual 定谳)· [yield_slice_return 提案](2026-11-08-yield-slice-return-proposal.md)(抬值语义的本刀边界补:环外让出者豁免)· [P22 exit 异步 ht_drain](2026-11-13-p22-exit-async-ht-drain.md)(自旋诊断首站)· 06 号 §3-4/§4-1/§4-4
