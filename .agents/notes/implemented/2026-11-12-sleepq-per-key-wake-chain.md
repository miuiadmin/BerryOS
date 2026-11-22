# Agent Note: SLEEPQ per-key 唤醒链（P2-1——同键等待免全队扫）

Status: implemented

## Problem

`wake_matching` 的唤醒资格判要找到「等在某个 channel/event 上的等待者」，P1 之前只有 SLEEPQ 全队线性扫一径：唤醒者持 SQ 锁全队 O(N) 走查，逐个比对 `wait_obj` 键匹配——被等对象（key）相同的那一小撮等待者（K 个）与全队人数（N）无关地拖累每一次唤醒。P1（28360b50）先落了 per-key 计数门控：无等待者时 O(1) 跳过全扫；但**有等待者时**仍退化为全队扫（计数 >0 ⇒ 必扫全队找那 K 个）。P2-1 把「找 K 个」本身降为 O(K)。

## Decision

- **Task 增同键兄弟链**：`key_prev`/`key_next` 双向链（`Cell<Option<NonNull<Task>>>`），与 SLEEPQ 主链**双挂**——同一把 SQ 锁下 insert 双插、unlink 双摘，不引入新锁序面。
- **64 桶哈希头**：`KEY_HEADS`（static mut，桶号复用 P1 `wait_bucket`——`KEY_BUCKETS == WAIT_BUCKETS` const assert 钉死两表对齐）。真对象键（channel/event 端点地址）等待者入桶；哨兵（`wait_obj` 为 0/纯睡形）与 WAIT_ANY 形不入链。
- **wake_matching 查桶**：key 等待走桶→同键链 O(K)，替代全队 O(N)；WAIT_ANY 全局兜底扫描保留（语义面：任意信号可醒）。
- **维护律**：`key_chainable` 判「真键等待者」；unlink 的覆盖写（`wait_obj` 被清后再摘链会扬链）由判据并上「桶头 == 自己」补单元素环；同点维护同桶对应律（`wait_obj` ⟺ 对应链非空）。
- **锁面**：`KEY_HEADS` 持裸指针非原子面——与 SLEEPQ 主链同一 SQ 临界区纪律（P1 前置同理），无新并发面。

## Consequences

- 同键唤醒（IPC ping-pong 主路径）从 O(N) 降为 O(K)：全队扫只在 WAIT_ANY 兜底与桶哈希退化（全碰撞）时出现。
- 后续 [`R3 形 B runnable 受捐注入`](./2026-11-12-pi-form-b-runnable-donee.md)（42a794e0）的 `pi_count_add`/受捐判定消费同一 P2 门控家族（①派生位等待者非零早退）。
- BLOCK/WAIT/CHAIN/MANY/PI/FAST 全绿（七十六轮）；wake_matching 行为面零漂移（同一批被唤醒者，只是查找路径变）。

## Alternatives considered

- **每对象一个 waitq + 超时队列全双挂**（经典双挂）：被既有设计否决（SleepQueue 类型注释）——Task 链接字段翻倍、双挂一致性易错；本刀同键链只在同键扫描面复用「双挂」收益，deadline 主序与超时扫描仍单队列，机械面收口不变。
- **哨兵也入链（专桶）**：哨兵挂 `wait_bucket(WAIT_ANY)` 专桶、每次唤醒扫两链，可把哨兵在场也压到 O(哨兵数)——被弃：与任务书「哨兵留全局、SLEEPQ 扫描兜底」的设计定谳相悖，且 v1 哨兵个位数，兜底冷路径不值得多一套专桶纪律。
- **退守 fastpath 单点优化**（任务书风险控制预案：只改 `find_single_waiter_locked`）：不需要——全链机械收口在两机械面后影响面可控，BLOCK/WAIT/CHAIN/MANY/PI/FAST 冒烟全绿。

## Related

- [系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md) P2-1（SLEEPQ 分桶）· §0 已实施基线（五十五轮 P1 门控行——本刀的前置半边）
- [sched/src/sched.rs](../../../sched/src/sched.rs) KEY_HEADS 块注（P2-1 机械面与哨兵兜底设计）· [sched/src/task.rs](../../../sched/src/task.rs) key_prev/key_next 字段注
- [wqmatch](../../../wqmatch/src/lib.rs)（谓词单源——键门拒冲突桶异键者是本刀正确性的宿主面）
