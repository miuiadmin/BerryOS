# 内核深度进化 20 号：kern_lock 双 guard 与 mm 三锁助手复活实装验证——第 1 步随刀序①②实跑定谳（20 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：19 号件把回流第 1 步随刀序实施化为①kern_lock②mm.rs③第 0 步④冲突段解——本件把①②两步从「fceb 形逐 hunk 草案」升为**隔离 worktree 实装编译验证**：在 base tip `24875ca57` 干净树上按 fceb 形逐 symbol 增量补回 `kern_lock.rs` 双 guard（`KlockIrqSaveGuard`/`klock_enter_irqsave` + `TableIrqSaveGuard`/`kern_acquire_irqsave_reentrant`，+55 行）与 `mm.rs` 三锁助手（`ALLOC_LOCK`/`AllocGuard`/`alloc_lock` + `with_frames`/`frames_locked` + `allocated_pages`，+65 行），`cargo check -p berry-kernel` 与 `--features lockdep` 双形零 error、hal crate 零 dead_code 警告——随刀序①②独立成刀可行性实证，且带出三个新定谳：mm 六面 dead_code 警告为**预期中间态**（消费者 heap/vmo/aspace/ktest 随回流后续步回来）、`just build` 的 build-user 红为 **owner main 线并行会话半成品债**（launcher inputfeed/HostHooks 面，base tip 原树同红 6 错实证，与回流供体复活零关）、unsafe 台账随源 +1（1214→1215，frames_locked 单 unsafe 块）。

## 审计基线（2026-11-20 05:1x 直核）

- base 线 = `origin/wip/sched-1.1@24875ca57`（19 号件本体笔即 base tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- fceb↔overhaul `kern_lock.rs`/`mm.rs` 零 diff 复核成立（18 号判据同款）；base tip 双 symbol/六面全零复核成立。
- 隔离验证树 = git worktree（detach 24875ca57），验证后已拆除，不污染主工作树。

## 主发现一：随刀序①kern_lock 双 guard 实装定谳——底座全活、原样插回即绿

- **插入点**：base tip `kern_exit()` 之后、`// ---- PRINT_LOCK ----` 分隔行之前，fceb `:293-:355` 块整段（63 行：双 struct 定义 + 双构造 fn + 双 Drop impl，含 doc 注释）。
- **底座核验**：双 guard 依赖的 `kl_held()`/`kern_acquire()`/`kern_release()`/`svc_enter_class()`/`svc_exit_class()`/`irq_save`/`irq_restore`/`disable_irq` 在 base tip 全活（:68/:94/:100/:153/:167 + lib.rs :397/:406/:413）——探针退役刀（2026-11-19）只撤 S-6 ③b 诊断账面（KL_HELD_SINCE/NAME_HOOK/bark 环），与双 guard 零耦合，diff 231 行全在退役面与注释，guard 块是纯减面。
- **编译验证**：`cargo check -p berry-hal-aarch64`（默认 + `--features lockdep`）零 error；双 guard 零 dead_code 警告（hal crate pub 面免检）；`#[cfg(feature = "lockdep")]` 臂随 base 侧 `CLASS_TABLE` 记账形（kern_acquire 内建）自动兼容。
- **语义注记**：`klock_enter_irqsave` 修的是 dpg::probe EL0 异常探针的 `eret` 前解蔽竞态（F0-3 面）；`kern_acquire_irqsave_reentrant` 修的是 `Vmar::Drop` 任意锁态收口点的重入退化（F0-2 面）——两守卫都是 13 号第 1 步 aspace/dpg 整取的随刀必需件。

## 主发现二：随刀序②mm 三锁助手实装定谳——两制兼容、六面 dead_code 为预期中间态

- **插入点两段**：use 区尾补 `use core::sync::atomic::{AtomicBool, Ordering};` + `ALLOC_LOCK`/`AllocGuard`/`alloc_lock`/`Drop` 块（fceb :35-:71，37 行）插 `use berry_mm::MAX_ORDER;` 后；`with_frames`/`allocated_pages`/`frames_locked` 三助手（fceb :236-:258，25 行）插 `frames()` doc 注释前。
- **底座核验**：`CLASS_ALLOC`（lockdep.rs :55/:153/:163）、`irq_save/disable_irq/irq_restore` base tip 全活；`FRAMES` static 与 `&raw mut` 取用形与 base 侧 `#![allow(static_mut_refs)]` 模块 attr 共存无新警告。
- **dead_code 六面定性（关键定谳）**：`ALLOC_LOCK`/`AllocGuard`/`alloc_lock`/`with_frames`/`allocated_pages`/`frames_locked` 在隔离树报 never used/never constructed——**预期中间态非债**：fceb 时消费在 heap.rs :60-:99（6 命中）与 vmo.rs :118-:395（6 命中），overhaul ktest :12247-:12266 另有 `mm::allocated_pages` 14 命中；这些消费者文件随回流第 1 步（aspace/vmo 整取）与第 4 步（ktest 并集）回来后警告自然消失。回流施工时此六面警告=复活完成的对照信号，不须 `#[allow(dead_code)]` 压制。
- **语义调和新定谳**：base tip `frames()` doc 已演进出「SMP 现役下帧分配器实际由 SVC TABLE 类 KL 窗串行保护，本文件无需自持锁」形——fceb ALLOC_LOCK 形（rank 5 自持锁）与 base KL 窗形是**两制并存兼容**非互斥：`with_frames` 闭包内取的是 ALLOC_LOCK（KL 窗内取 ALLOC_LOCK = rank 5 在 rank 4 之内，lockdep 序合法）；回流后消费者切换 with_frames 形时 KL 窗串行语义不变（ALLOC_LOCK 只是更细粒度）。回流实施时**不删 base 侧 frames() doc 的 KL 窗注记**——两制注记并存，消费面切换完成后由后续演进刀收口。

## 主发现三：`just build` build-user 红 = owner main 线并行会话半成品债（与回流无关）

- 隔离树 `just build` 红：`berry-launcher` 4 错（`mod inputfeed` E0583 文件不存在 + `on_input`/`aux_handles`/`on_aux` E0407 非 HostHooks trait 成员）。
- **base tip 原树对照实跑**（stash 我的改动后同面）：同 6 错——红源为 owner 386066c6f（main 线 merge 笔）明言「并行会话半成品（inputfeed/on_input/aux_handles）未入主线」，但该笔在 main 不在 wip/sched-1.1（`merge-base --is-ancestor` 非 ancestor 实证）；wip 侧 main.rs :59 `mod inputfeed;` 声明在而 inputfeed.rs 全历史零落库（`-S` 全历史零命中）——与 9dd854876 I3 断链债同族（main 线半成品引用泄漏），**非本刀引入非回流面**，登记为 base 债待 owner 清偿（修法二选一：wip 侧摘 :59 mod 声明 + frameloop 三方法摘除，或补齐 inputfeed.rs——半成品本体在 owner 手）。

## 复活施工卡（第 1 步①②两刀实施化，逐 hunk）

- **刀①kern_lock.rs**：单插入点一处（PRINT_LOCK 分隔行前），63 行整段无手工调和点——底座全活零依赖冲突，fceb 形即终形。
- **刀②mm.rs**：两插入点（use 区尾 + frames() 前），62 行整段；唯一手工点 = 台账随源重产（+1 unsafe 块 = frames_locked 内 `(*(&raw mut FRAMES)).as_mut()`），`python3 tools/unsafe_audit.py` 重产 1214→1215 条 `--check` 对拍 ok 实证；wg 面 `ALLOC_LOCK` 为 AtomicBool 非 static mut，writable_globals 91 条不变。
- **验证口径**：两刀 `cargo check -p berry-kernel`（默认+lockdep）0 error 即达门；test-host 不涉（kernel crate 无 host 测增量）；`just build` 债面见主发现三（base 自身红，不阻回流刀合并判断——CI light 面 kernel check 已覆盖）。

## 与其他文档的关系

- 随刀序单源：[19-第六连E0425合并树实证-供体复活随刀序实施化与vmar六调用点表述修正.md](19-第六连E0425合并树实证-供体复活随刀序实施化与vmar六调用点表述修正.md)——其①②步实施化由本件实装验证背书，③④步仍为草案。
- 判据单源：[18-事故族第四面排查定谳-7f05be12e全symbol扫描收口-6文件18symbol配套静默回退与消费链连带丢失.md](18-事故族第四面排查定谳-7f05be12e全symbol扫描收口-6文件18symbol配套静默回退与消费链连带丢失.md)——本件覆盖其 kern_lock 双 symbol 与 mm 三锁助手五面；余面（lib.rs 双函数/virtio 窗重置/trap iabort 对/tlbshoot/vmoledger 六面）待③步与第 1/4 步随刀。
- 供体清单单源：[17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md](17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md)——第 0 步草案不变，本件①②实证为 19 号随刀序前两步的实施化背书。
- 证据可复现：worktree 验证 = `git worktree add --detach <dir> 24875ca57` 后按本件施工卡两插入点贴块，`cargo check -p berry-kernel{, --features lockdep}` 0 error；dead_code 对照 = fceb `git grep -c 'with_frames|frames_locked|alloc_lock' fceb03ff3 -- kernel/src/heap.rs kernel/src/vmo.rs`（6/6）；build 债对照 = base tip 原树 `just build-user` 同 6 错；台账 = `python3 tools/unsafe_audit.py` 重产 1215 `--check` ok。
- 沿革：`docs/阶段沿革.md` 六百二十六节（随本批）。
