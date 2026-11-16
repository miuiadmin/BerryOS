# Intent M4 透传:svcmgr app spawn → 内核 Task 字段(sched_setattr 活面接管 dormant 口②)

Status: implemented

## Problem

Intent M4 骨架(带定义/实例化面/policy band 位)已落,但 manifest 声明的 intent 在 app spawn 链上断在第三段:manifest 声明(appdemo=utility 0x03/appquiet=background 0x04)→ berry-manifest 解析(`Manifest.intent`,判别值镜像 `sched/src/intent.rs` 六档)→ svcmgr `app_launch` 读到即弃(`SpawnSpec.intent` 硬编码 0,旧注「解析面记录、调度零消费」)→ 内核 Task 的 `intent_declared/effective` 恒为 bootparams 缺省。owner 窗(骨架注预留窗④:task→band 赋值)开窗前,值必须先流进内核——否则带实例化查表键无数据源。

链路侦察另发现 SDK 透传口②(GUI-8 §4 段①)的落点假设已过时:口②把 intent 塞进 `berry_thread_create` options 低 8 位,但内核从未开放 options 面(`sys_thread_create` 对 options≠0 恒 -EINVAL,非 0 调用即自陷);调度 1.1 实际落的活面是 `sched_setattr`(0x3C)——Thread 覆盖位在 `process_start`/`thread_start` 经 `SchedAttr::merge` 消费进 Task 双字段(`apply_sched_attr_to_task`)。dormant 口②的「内核收 intent 后自然激活」永远等不到,options 路线是死枝。

## Decision

三件(user 侧零内核/sched/arch 改动):

1. **berry-exec 激活口②换轨活面**:`spawn_rootfs_elf` 内 `berry_thread_create(hp, 0, ...)` options 回恒 0(内核契约面);`spec.intent ≠ 0` 时在 thread_create 后、process_start 前发 `berry_sched_setattr(ht, &SchedAttr{flags: SCHED_FLAG_INTENT, intent, ...})`。选 Thread 覆盖位而非 Process 默认位的结构理由:`process_create` 出生授 Process 的是 MANAGE|DUPLICATE|TRANSFER(无 SET_POLICY,setattr 即 -EACCES),而 `thread_create` 出生授 Thread SET_POLICY|INSPECT(syscall.rs:1530-1533)——ht 是调用方唯一自带执法权的落位。start 前落位 = 随 `merge(proc, thrd)` 进 start 处消费,Task 双字段由此钉进 TCB。intent=0 零新 syscall 零漂(既有 22 构造点全 0)。
2. **svcmgr `app_launch` 传值**:`SpawnSpec.intent = man.intent as u8`(形状钉子:berry-manifest Intent 判别值 day-one 冻结镜像 sched 六档,`as u8` 即正确编码)。
3. **读回观测行(验证面)**:spawn Ok 后 `berry_sched_getattr(child.ht)` 读内核侧 effective intent(与 process_start 消费进 Task 的是同一份 merge 值)→ eco 诊断行 `eco: app <id> it XX`(十六进制档位)。内核状态读回非自回声——行值≠manifest 声明即断链 trap(A-3 口径;ht 恒持 INSPECT)。

## Alternatives considered

- svcmgr 侧 spawn 后对 child.ht setattr(否:apply_sched_attr_to_task 只在 start 时消费,post-start setattr 只改 Thread 对象存储不动运行中 Task——假透传)。
- svcmgr 自建 spawn 链绕开 berry-exec(否:「不长第二套」立法;且 app_launch 全部 gift/授权装配在 SpawnSpec 面上,重写即双表)。
- 动内核给 thread_create options 开 intent 位(否:内核已有设计好的 0x3C 面,重开 options 位 = 两张门;且能不动内核就不动)。
- 观测行改 ps 账本/proc 面(否:psproto 无 intent 字段,扩协议面远超本刀;eco 诊断族即最小可查面)。

## Consequences

- QEMU rootfs 实跑(rootfs0/1 x62 条目,9 app):`eco: app APPDEMO it 03`/`APPQUIET it 04`/七个 utility app 全 `it 03`,`APP ok n=9` 在席——manifest→parse→SpawnSpec→setattr→merge→Task 双字段→getattr 读回全链闭合。boot 相关键标记(SVC/DEMO/ECO/STD/HEXYL/BSH/EXIT/PIPE/FS/VIRTIO/VBLK/BLKD/NET)逐一到齐,bsh 面(intent:0 族)零漂实测。
- smoke 无新 gate:eco 行属诊断族不进标记;smoke 断言全是存在性 grep,新增行零干扰。
- berry-libc pthread 口①(thread_create options 位)仍 dormant 且内核面永远不会开——后续若有 pthread intent 需求,同样换轨 sched_setattr(子进程内对自建线程 setattr,首线程则靠本刀链)。
- owner 窗(骨架注预留窗④)开窗时数据已就位:spawn_intent 接线点读 Task.intent_declared 即得 manifest 真值;`band_id_of_intent` 恒等映射直接可用。
- 范围裁量说明:任务书言「只改 svcmgr 侧的传递面」,本刀含 berry-exec 一处——spawn 窗口(create→start)结构性封装在 berry-exec 内,svcmgr 无插入点;berry-exec 即 svcmgr 与内核 spawn 调用之间的传递面本体,且不属 owner 占用域(sched/arch)。备择「纯 svcmgr + 记内核侧参数位待 owner 窗」不成立:内核参数位(0x3C)已在,缺口在 userland 管线。

## Related

[Intent M4 带骨架纯面](2026-11-15-intent-m4-band-骨架纯面.md)(预留窗④ task→band 赋值 = 本刀下游 owner 窗);`docs/架构决策.md` ADR #42;`research/调度彻底计划/03-彻底计划骨架-刀序里程碑验证体系-v1.0.md` CP-12;SDK 透传三口史:`docs/阶段沿革.md` GUI-8 段① 段② 三工之 C。
