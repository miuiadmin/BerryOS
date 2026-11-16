# Agent Note: SLEEPQ per-key 唤醒链——同键兄弟链替「有等待者」全扫（路线图 P2-1）

Status: implemented

## Problem

`wake_matching` / `find_single_waiter` 在 SLEEPQ 有等待者时仍全队列扫描 O(全体睡眠者)。五十五轮 P1 wait_gate（WAIT_HASH 64 桶 per-key 计数）只解决「无等待者」的 O(1) 跳过——高线程数（百级+）下多数睡眠者睡的是别的键，每次 channel_send/signal/wake 的匹配成本仍随全体睡眠者线性涨（[系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md) P2-1，R1 下一代 #3 五十九轮终局遗项）。

## Decision

- **同键兄弟链与 SLEEPQ 双挂，不改全局 SLEEPQ 结构**：`Task` 追加 `key_prev`/`key_next` 两 Cell 字段（追加到 struct 末尾，现有字段偏移不动）；全局 SLEEPQ 仍是唯一的 deadline 升序面（超时扫描 `expire_timeouts` 头部早停零变化），`KEY_HEADS`（64 桶 static mut 链表头，`tools/writable_globals.txt` 入册）只是同键等待者的第二挂点。
- **桶号复用 P1 `wait_bucket`**（Fibonacci hashing），`KEY_BUCKETS == WAIT_BUCKETS` const 断言钉死——计数桶与链桶同号，维持「WAIT_HASH 桶非零 ⟺ 对应链非空」的同桶对应律，门控读数与扫描面天然对齐。
- **维护收口在 `SleepQueue::insert_sorted`/`unlink` 两机械面**（SLEEPQ 成员变更仅有两入口——与 P1 计数、P2 PI 计数同点同纪律）：入队接线、离队摘线，无第三写点；`insert_sorted` 入口并加键链在队执法 assert（判据并上「桶头 == 自己」补单元素链盲区，同 SLEEPQ 执法面）。
- **键链序 = SLEEPQ 序的同键投影**：插链比较器与 SLEEPQ 逐字相同（deadline 升序、相等插尾→同 deadline FIFO）。这使同键链遍历序与旧全扫的命中序一致——唤醒顺序（入 runqueue 序）零漂移，纯性能改动、无行为面。
- **WAIT_ANY 哨兵不入链**：哨兵命中任意 obj 的唤醒，单键链罩不住——留全局 SLEEPQ，`WAIT_ANY_WAITERS` 非零时 `wake_matching_locked`/`find_single_waiter_locked` 退回全 SLEEPQ 扫描（P2-1 前原路径逐字保留，冷路径）；纯睡（wait_obj=0）永不匹配任何唤醒，同不入链。
- **无哨兵主路径只扫同键链**：门开（wait_gate 未清）且 `WAIT_ANY_WAITERS == 0` 时，非哨兵匹配者的键门唯一形是「obj 相等」，全在键桶链上——扫描面 O(全体睡眠者)→O(同桶)；冲突桶异键者由 `berry_wqmatch::matches_wake` 谓词键门拒（只许假阳性、语义零漂移，P1 粗计数同论证）。
- **零新增 unsafe 出现点**：键链手术内联在两机械面（`insert_sorted`/`unlink` 均已是 unsafe fn、体内直写裸指针是本文件既有风格），扫描分支收在 `wake_matching_locked`/`find_single_waiter_locked` 既有的 unsafe 块内——`docs/unsafe台账.md` 不动（该台账在本分支本就有他人未重产的漂移，见 Consequences）。

## Alternatives considered

- **每对象一个 waitq + 超时队列全双挂**（经典双挂）：被既有设计否决（SleepQueue 类型注释）——Task 链接字段翻倍、双挂一致性易错；本刀同键链只在同键扫描面复用「双挂」收益，deadline 主序与超时扫描仍单队列，机械面收口不变。
- **哨兵也入链（专桶）**：哨兵挂 `wait_bucket(WAIT_ANY)` 专桶、每次唤醒扫两链，可把哨兵在场也压到 O(哨兵数)——被弃：与任务书「哨兵留全局、SLEEPQ 扫描兜底」的设计定谳相悖，且 v1 哨兵个位数，兜底冷路径不值得多一套专桶纪律。
- **退守 fastpath 单点优化**（任务书风险控制预案：只改 `find_single_waiter_locked`）：不需要——全链机械收口在两机械面后影响面可控，BLOCK/WAIT/CHAIN/MANY/PI/FAST 冒烟全绿。

## Consequences

- 高线程数下唤醒匹配 O(N)→O(N/桶)（N = 全体睡眠者；桶内还叠加「同键才入链」的精确过滤）；多服务/线程 churn 规模化护航（路线图 P2-1 预期收益兑现）。
- `Task` 增 16B（两个 Cell<Option<NonNull>>）；`KEY_HEADS` 64×8B static（.bss）。
- SMP 实验态正确性由既有纪律承载：KEY_HEADS 读写点全在 SQ 锁窗内（与 SLEEPQ 本体、WAIT_HASH 同论证——锁交接即可见性边界），无新增跨核面。
- `docs/unsafe台账.md` 的 `--check` 在本分支改刀前即红（kernel/ 计数 24↔25 与 arch lib.rs 内容锚漂移，来自工作树他序改动；本刀 sched 侧零新增 unsafe 出现点、不重产台账以免吸收他人漂移）——合并前需由漂移归属方重产。
- 路线图 P2-1 状态翻转（§0 已实施基线表）未随本刀改动（任务书限定改动面 sched/src/task.rs、sched/src/sched.rs 与 writable_globals.txt）；本笔记即完成事实记录，翻表归合并批。

## Related

- [系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md) P2-1（SLEEPQ 分桶）· §0 已实施基线（五十五轮 P1 门控行——本刀的前置半边）
- [sched/src/sched.rs](../../../sched/src/sched.rs) KEY_HEADS 块注（P2-1 机械面与哨兵兜底设计）· [sched/src/task.rs](../../../sched/src/task.rs) key_prev/key_next 字段注
- [wqmatch](../../../wqmatch/src/lib.rs)（谓词单源——键门拒冲突桶异键者是本刀正确性的宿主面）
