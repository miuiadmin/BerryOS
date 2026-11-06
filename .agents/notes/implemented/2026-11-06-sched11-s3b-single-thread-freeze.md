# 调度 1.1 S3-b：SINGLE_THREAD 冻结与 FREEZE_READY 自证

Status: implemented

## Problem

`process_suspend` 只支持进程全冻：`options=SINGLE_THREAD` 恒 `-ENOTSUP`，Thread 无法作为冻结单位；用户态锁不可见，直接冻结任意线程会把同进程其他线程留在永久等锁状态。既有 `SuspendTok` 也只记录 Process 与 `pinned`，无法承载线程级 token。

## Decision

- `process_suspend(options bit0=SINGLE_THREAD)` 只接受 Thread，并要求目标先经 `sched_setattr(FREEZE_READY)` 自证用户锁已到安全点；最后一次 thaw 由内核清除该位，下次冻结必须重新自证。
- `SuspendTok` 目标改为 `Process | Thread`：Process 按 `pinned` 冻全部任务，Thread 按 `Task.thread_key` 定位唯一任务；两种目标各自维护 depth，最后一个 token close 才 thaw。
- 冻结扫描覆盖 SLEEPQ 与两个 per-CPU Runqueue。RQ 是环形链，扫描必须先处理当前节点再判断回到 head，避免尾节点或单元素环漏访。
- 一个 `HELD_FROZEN` 位不能同时表达「进程 token + 线程 token」的交叉持有；Process 记录 `thread_freezes` 占位，进程全冻与单线程冻在 1.1 互斥，防止先关闭的一侧提前解冻另一侧。
- `thread_create` 初始授 `SET_POLICY|INSPECT`，创建者可声明 attr/FREEZE_READY 并查询 effective 属性；能力仍可 duplicate 收紧或委托。

## Alternatives considered

- **允许进程全冻与单线程冻重叠**：被否决——两种 token 共用同一个 `HELD_FROZEN` 位，先归零的一侧会把另一侧仍持有的冻结清掉；拆两位会占用后续 obligations/锁义务位并扩大 Task 布局变化。
- **只按 Thread 对象加 frozen 布尔**：被否决——调度器消费的是 Task；冻结位已在 `Task.held_locks`，再引入第二来源会产生位图与布尔漂移。
- **不要求 FREEZE_READY**：被否决——内核看不见 pthread mutex/futex；无自证冻结会把「冻结一个线程」变成同进程死锁发生器。

## Consequences

- `thrchild` 在原 T-1 双线程链后新增第三线程：设置 READY → 启动 → 单线程冻结 → 查询 READY → 关闭 token → READY 自动清除 → 放行并 join，`THR ok` 同时成为 S3-b 用户态活证。
- `FREEZE-K` 覆盖嵌套 token、末 token 清 READY、Process 单线程占位归零；ABI 0x3E `implemented=true`，berry-sys 提供 `berry_process_suspend` 与 `SUSPEND_OPT_SINGLE_THREAD`。
- 不覆盖项：`obligations` 位图、SMP IPI 安全点协议、冻结时钟/预算暂停语义仍属后续调度 1.1 余项。
- 环形 RQ 扫描漏访尾/单元素节点的问题定谳为雷册 §A136：终止检查必须位于当前节点处理之后。

## Related

- [调度 1.1 判据笔记](2026-11-06-agents-md-rollback-sched11.md)
- `docs/调点调度-v1.1.md` §5/§7
- `docs/雷册-判例库.md` §A136
