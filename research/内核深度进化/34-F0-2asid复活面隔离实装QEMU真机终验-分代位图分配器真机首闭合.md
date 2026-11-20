# 内核深度进化 34 号：F0-2 asid 复活面隔离实装 QEMU 真机终验——分代位图分配器真机首闭合（34 号件）

一句话说明：15 号定谳了 F0-2 全家面 base 线静默回退（分代位图分配器/取号窗/drop 归还接线全链随 7f05be12e 丢入合并树域）、21 号验了第 0 步七点隔离树编译绿（四连红全消）、33 号合树跑通了第 0 步＋27 号七刀的 QEMU 真机——但**纯 F0-2 面（asid.rs 整取＋aspace 取号窗＋syscall 消费窗＋vmar drop 归还＋mmu F0-2 面）的真机回归从未跑过**（21 号只验到编译绿，33 号七刀树不含 F0-2 消费面）。本件闭合这个洞：base tip `86eefa6e3` 干净隔离树贴 F0-2 十文件增量（根 members＋arch dep＋mm dep＋asid.rs 整取＋aspace 取号窗＋syscall 消费窗＋vmar drop 归还＋mmu F0-2 面＋lib.rs re-export＋memface 本体〔随批，21 号第 8 触点〕），`cargo build -p berry-kernel` EXIT=0、lockdep 双形 0 error、台账随源重产 1215→1229 `--check` ok（+14 = memface 本体笔，21 号件口径吻合）、wg 91、docs-guard/abi-check PASS——**QEMU 真机跑通 `ktest done`（:120）＋68 个 boot 标记全到（含 BENCH ok，比 33 号件 29/30 更全）＋ `ASPACE ok (asid=16b)` 宽度探测面真机过＋expected D-abort 恢复链正常**；host 挂测面 mm 56 测全绿（asid.rs 挂测刷新形）＋asidface 5 测全绿；并带两个边界定谳：①build-user 红 = 20 号在案 E0407 HostHooks 债（render_hold 同指纹，非本刀引入——本刀面 syscall/vmar/aspace/mm/asidface 零 launcher 触点）；②对照红预测属合并树域（19/21 号已证），base 增量贴入形无对照红义务。

## 1. 背景与验证留白

- 15 号：F0-2 全家面 base 线静默回退定谳（try_alloc/free_asid/active_cpu_asids/挂测行孤儿四连红清单）。
- 21 号：第 0 步七点（含 F0-2 面）隔离树 cargo build EXIT=0——编译绿已证，真机未跑。
- 33 号：第 0 步＋27 号七刀合树真机 ktest done 首闭合——但七刀面是 kern_lock/tlbshoot/mm/vmo/abi_gen/ktest/memface/mmu F0-1·F0-4，**不含 F0-2 消费面**（asid.rs 整取与取号窗仍在 21 号树未跑真机）。
- 本件验证口径：F0-2 复活十文件在 base 净树隔离贴入后的 QEMU 真机回归——ASID 分配器是地址空间生命周期的心脏（process_create 取号/销毁还号），分配器语义错误 = 撞号击穿 TLB 隔离或耗尽 panic，均应在 boot/ktest 相暴露。

## 2. 施工资序（十文件，w3-21 证据树平移）

隔离 worktree：`git worktree add --detach ~/.hermes/cache/scratch/w3-34 86eefa6e3`（base 净树）。

1. **根 Cargo.toml members**：`"asidface",` 一行（memface 行后）——21 号树原笔平移。
2. **arch/aarch64/Cargo.toml**：`berry-asidface = { path = "../../asidface" }`（memface 行后）——同上。
3. **mm/Cargo.toml**：`[dependencies]` + `berry-asidface` 行（原「无外部依赖」注释退位）。
4. **mm/src/asid.rs 整取**：轮转计数器形 → berry-asidface `AsidState` 分代位图形（AsidAllocator 包装层：new/try_alloc(pinned)/alloc()/free/active_count/generation/force_near_roll 挂测面刷新）。
5. **kernel/src/aspace.rs 取号窗**：`alloc_asid()` → `try_alloc_asid()`（pinned = active_cpu_asids() 钉死面）＋便捷形 alloc_asid()（expect 形，boot/ktest 语境）＋`free_asid()`（归还；0/重复释放空操作）。
6. **kernel/src/syscall.rs 消费窗**：`sys_process_create` 取号段改 try 形——耗尽诚实返 -ENOMEM（绝不撞号）＋建表失败臂立即 free_asid 归还。
7. **kernel/src/vmar.rs drop 归还**：`Drop for Vmar` 尾加 free_asid 归还（asid != 0 才还；v1 不回收表树帧政策不变，ASID 是唯一随 Drop 归还的硬件资源）。
8. **arch/aarch64/src/mmu.rs F0-2 面**：`use berry_asidface::{tcr_as_config, ASID_SHIFT}`＋`ACTIVE_CPU_ASID: [AtomicU16; 2]`＋`active_cpu_asids()`（Acquire 读）＋`asid_bits()` 改 tcr_as_config(v).0＋`asid_shift()` const 化（ASID_SHIFT=48，修 8 位错移 56 缺陷）＋`activate_ttbr0_regs` 内 Acquire/Release 记账＋CONTEXTIDR 写后 isb 补序。
9. **arch/aarch64/src/lib.rs re-export**：`active_cpu_asids()` 一笔（F0-1/F0-4 四 re-export 随批在场——33 号件已合 base，此树贴入形自动全绿）。
10. **memface/src/lib.rs 本体**（随批，21 号第 8 触点）：base 空壳 → 满血本体（cache_line_span/ctr_dmin_line/ctr_pou_plan/IcachePolicy）——33 号件入 base 的是 memface 面，本树随批再贴一次保证独立性。

施工细节：w3-21 树 diff 按 8 文件平移（mmu.rs/lib.rs 全量 diff 含 F0-1/F0-4 面随批——base 33 号件已合入同面，3way apply 自动并），memface 本体 94 行独立贴入。**零冲突净贴**。

## 3. 六门实测

| 门 | 结果 | 注 |
| --- | --- | --- |
| cargo build -p berry-kernel | EXIT=0 | 27 警全 base 既有对照 |
| lockdep 双形（check --features lockdep） | 0 error | 单元计数不漂 |
| unsafe 台账（unsafe_audit.py 重产 + --check） | 1229 条 ok | 1215→1229，+14 = memface 本体笔 |
| writable-globals | ok（91 entries） | 不变 |
| docs-guard | PASS（0 警） | G10 201=201 |
| abi-check | PASS | 0x1C WARN = 27 号刀④义务面（在案） |

## 4. QEMU 真机门（本件主验面）

命令：`timeout 220 qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel $(bash tools/paths.sh kernel) < /dev/null`（内核相直跑，33 号件同款）。

- **`ktest done` 到达（:120）**——F0-2 面真机首闭合。
- **68 个 boot 标记全 ok**（33 号件 29/30；本树 BENCH ok n=10 也到了——churn 相在 220s 窗内跑完，非缺项常态）。
- **`ASPACE ok (asid=16b, low-half faulted)`（:14）**——ASID 宽度探测（tcr_as_config 面）真机过；16 位形态下 ACTIVE_CPU_ASID 记账面真机激活。
- **expected D-abort 恢复链正常**（E2 旧形 RO 写/unmap 读/切分中页 fault 族）——ASID 复活未搅动 fault 恢复语义。
- 无 panic/无 FAIL/无 allocation failure。

## 5. host 挂测面

- `cargo test -p berry-mm --target aarch64-apple-darwin`：56 passed（asid.rs 挂测刷新形：width8/width16 零保留/force_near_roll 挂测族）。
- `cargo test -p berry-asidface --target aarch64-apple-darwin`：5 passed（tcr_as_config/AsidState 纯函数面）。

## 6. 边界定谳

1. **build-user 红 = 20 号在案 E0407 HostHooks 债**：berry-launcher render_hold E0407 同指纹（33 号件同款记录）；本刀文件面（syscall/vmar/aspace/mm/asidface）零 launcher 触点，非本刀引入。abuser/svcmgr bin 已产出，内核相直跑验面已覆盖；smoke 完整相（rootfs）不可跑如实记录。
2. **对照红补验豁免依据**：19/21 号已证合并树域对照红（E0425/E0432 四连+第五连），本树为 base 增量贴入形（净贴零冲突），无对照红义务。
3. **⑥债复核**：user/Cargo.toml members 仍无 berry-hal（base 级在案债，worker 不自发清偿）。

## 7. 与既有文档的关系

- F0-2 回退定谳单源：[15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md](15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md)——本件闭合其真机验证留白（编译绿 → 真机绿）。
- 第 0 步施工单源：[17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md](17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md)——刀④⑤（asid.rs 整取＋取号窗）的真机实施验证。
- 编译验证单源：[21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)——本件为其实机续验件；台账 +14 口径吻合。
- 合树真机单源：[33-第0步与27号七刀合树隔离实装QEMU真机终验-ktestF0-3F0-4自检面真机首闭合.md](33-第0步与27号七刀合树隔离实装QEMU真机终验-ktestF0-3F0-4自检面真机首闭合.md)——七刀面真机已闭合，本件补 F0-2 消费面；memface 本体在本树为随批笔（33 号件已合 base）。
- 证据可复现：worktree `git worktree add --detach <dir> 86eefa6e3` 后平移 w3-21 diff（8 文件）＋memface 本体（94 行）；build = `CARGO_TARGET_DIR=<独立> cargo build -p berry-kernel`；台账 = `python3 tools/unsafe_audit.py`（1229 条）；真机 = 上述 QEMU 命令——`ktest done` 即 F0-2 面全过标记；host 挂测 = `cargo test -p berry-mm --target aarch64-apple-darwin`（56 测）。

## 8. 结论

F0-2 asid 复活面（15 号定谳、17 号施工图、21 号编译绿）的真机验证留白至此闭合：十文件净贴 base tip 86eefa6e3，六门绿，QEMU 真机 ktest done＋68 标记＋ASPACE asid=16b 宽度面，host 挂测 61 测全绿。分代位图分配器在真机形态下无撞号、无耗尽 panic、无 TLB 隔离击穿信号——回流的 F0-2 面真机就绪，可随回流主批合龙。
