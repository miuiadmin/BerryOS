# 内核深度进化 33 号：第 0 步＋27 号七刀合树隔离实装 QEMU 真机终验——ktest F0-3/F0-4 自检面真机首闭合（33 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：21 号验证了第 0 步七点增量补丁的编译绿、27 号把复活义务升为七刀同批（ktest 定义体↔五文件供体，分批即 E0425/E0432）、31 号纸面核了七刀与 E2 补丁零耦合——但**七刀合树（第 0 步供体面 + ktest 定义体 + 段 1 并集 + abi_gen 常量族同树一批）的 QEMU 真机回归从未跑过**（27 号验证口径「QEMU smoke ktest F0-3/F0-4 自检面随全量回归」留白至今）。本件闭合这个洞：base tip `3f9ec497a` 干净隔离树贴 22 文件合批增量（kern_lock 双 guard/tlbshoot 探针/mm allocated_pages/vmo contig·global 面/abi_gen 常量族/ktest f0_3·f0_4 定义体＋段 1 五调用/memface 本体/mmu.rs F0-1·F0-4 面），`cargo build -p berry-kernel` EXIT=0（27 警告全 base 既有对照）、lockdep 双形零 error、台账随源重产 1228→1232 `--check` ok、wg 91、docs-guard/abi-check PASS——**QEMU 真机跑通 `ktest done`（f0_3/f0_4 系 run() 尾五调用，纯断言面无专属输出行，断言失败即 panic 中断——done 到达即自检面全过）＋ 29/30 boot 标记（缺 `BENCH ok n=10` = TCG churn 相 220s 超时未等到，ktest done 已过，30 号同款非故障）＋ expected D-abort 恢复链正常**；并带三个边界定谳：①30 号 E2 补丁在 base 树不适用是正确行为（base 已有旧形 E2〔Q110 游标形〕合法在场，E2 重钉属回流合并树域替换施工非净插入——31 号「任意序」的适用域澄清），②build-user 红 = 20 号在案 base 债（E0407 HostHooks 面同指纹）致 smoke 完整相〔rootfs〕不可跑的如实记录，③abi-check 0x1C enforced_since WARN = 27 号刀④已知义务面（门控＋登记执法起始日）非本树引入。

## 审计基线（2026-11-20 13:46 轮直核）

- base 线 = `origin/wip/sched-1.1@3f9ec497a`（#604 合流后 tip；13:3x 轮 WIP 停靠点即此树）；隔离 worktree = `~/.hermes/cache/scratch/w3-33`（独立 `CARGO_TARGET_DIR`，32 号教训）。
- WIP 停靠形态：22 文件 modified 未 commit（arch 四文件 + kernel 五文件 + memface/vmoledger 本体 + abi 三产物 + 台账），diff 总量 +1581/-536（台账重产占大半）。
- 21 号第 0 步七点 vs 本树的差异定谳：本树不含 F0-2 asid 面（asid.rs 整取/syscall 取号窗/syscall 建表失败臂归还）——本树是 **27 号七刀面**（kern_lock/tlbshoot/mm/vmo/abi_gen/ktest/memface 本体）+ mmu.rs F0-1·F0-4 面的合批形；台账增量 +4（1228→1232，本刀 mmu.rs 函数面）vs 21 号 +14（七点全量含 asid/syscall 面）——两形口径不同数字各证。

## 验证链（六门全绿）

1. **编译门**：`cargo build -p berry-kernel`（aarch64-unknown-none 真链）EXIT=0，6.16s；27 警告经对照全为 base 原树既有（splash/channel/telemetry/vdso 面）。
2. **lockdep 门**：`cargo check -p berry-kernel --features lockdep` 零 error。
3. **台账门**：`python3 tools/unsafe_audit.py` 随源重产 1232 条（页表 93/上下文切换 35/MMIO 164/分配器 17/硬件原子 0/其它 923）`--check` ok——本刀增量 +4（mmu.rs F0-1/F0-4 函数面的 asm/unsafe 块系登记）。
4. **wg 门**：`writable_globals --check` ok（91 entries；新 statics 全 Atomic 形）。
5. **docs/abi 门**：`just docs-guard` PASS（0 警，G10 201=201）；`just abi-check` PASS（NR v1 42 号五面一致）——**0x1C vmo_cache_op enforced_since WARN 系 27 号刀④已知义务面**（门控＋登记执法起始日随回流施工），WARN 不拦 PASS，本树如实携带。
6. **QEMU 真机门（本件主验面）**：内核相直跑（`-kernel ELF` 直加载）——`ktest done` 到齐、29/30 boot 标记全到（唯一缺 `BENCH ok n=10`：churn 相在 220s timeout 窗内未跑到，`ktest done` 已过故非断言失败；30 号 smoke 超时同款常态）、expected D-abort 恢复链正常（E2 旧形 RO 写/unmap 读/切分中页 fault 族）。

## 主发现一：ktest F0-3/F0-4 自检面真机首闭合（27 号留白洞）

- 27 号施工卡验证口径末句「QEMU smoke ktest F0-3/F0-4 自检面随全量回归」——21 号只验到编译绿（cargo build EXIT=0），七刀树真机跑从未执行；本件补上。
- f0_3/f0_4 系纯断言面（`fn test_f0_3` 入口即 `assert_ne!(daif_before & 0x80, 0)`，无 println 输出行）——串口标记法不可见，唯一真机证据 = `ktest done` 到达（run() 尾调用序含 `test_f0_3(); test_f0_4();`，任一断言失败 panic 中断 ktest，done 必缺席）。
- 真机证据链：`ktest done` :296（日志 298 行）+ 前置 29 标记全到 + expected D-abort 恢复族正常——**F0-3（kern_lock 双 guard 临界段 DAIF 配平）与 F0-4（vmo contig/global 面＋tlbshoot 探针）在真机全过**。
- 判普登记：**纯断言 ktest 面的真机验证口径 = done 标记到达律**（无专属输出行的自检面，标记链即断言链）；给断言面加 println 观测行是反模式（污染标记序）。

## 主发现二：30 号 E2 补丁在 base 树不适用是正确行为（31 号「任意序」适用域澄清）

- 30 号补丁（`30-e2-repin-isolated.patch` 105 行）`git apply` 于本树失败：锚上下文 `:3309`（overhaul ktest 形）在 base 树对应位 `:3519`——**base 树已有旧形 E2 在场**（Q110 游标形三页跨段例，2026-11-19 解堵刀后的合法在场资产），补丁的插入点（锚后净插 94 行）在 base 树是已占用区。
- 定谳：31 号「E2 补丁与七刀任意序可贴」的适用域 = **回流合并树**（vmar 十段整取后 ktest 形趋同 overhaul，旧形 E2 消亡、重钉补丁的净插入点成立）——不是 base 树（旧形在场，重钉=替换施工）。31 号「E2 锚与符号面均在 base 既有树活」指锚断言行与消费 symbol 在场（`:3519` 恰 1 命中，本轮复核零漂），非补丁可贴。
- 本树（base 增量贴入形）不触发 E2 重钉义务——旧形 E2 就是当前合法形态（本轮 QEMU expected D-abort 族即旧形打的）。

## 主发现三：build-user base 债挡 smoke 完整相（20 号债面复现＋⑥债在场）

- `just build-user` 红：`berry-launcher` E0407 ×N（`render_hold`/`on_input`/`aux_handles` is not a member of trait `appshell::pump::HostHooks`）——与 20 号在案 base 债同指纹（owner main 线并行会话半成品，inputfeed/HostHooks 面，base tip 原树同红实证）。非本树引入（本树零触碰 user/）。
- 连带：smoke 完整相（rootfs cpio 注入 + G-eco 编译侧标记）不可跑——内核相直跑已覆盖本件验面（ktest/断言/恢复链），完整 smoke 留给回流终验（30 号 BENCH 标记滚新刀同批）。
- ⑥债复核：`user/Cargo.toml` members 仍无 `berry-hal` 行（reviewer LOG 13:1x 轮核验同结论）——base 级 test-host 连坐红持续，待 owner 放行清偿件。

## 与其他文档的关系

- 七刀单源：[27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md](27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md)——本件闭合其验证口径留白（QEMU 真机回归）；其刀⑤随批笔以 31 号勘误口径为准（30 号补丁原样）。
- 第 0 步编译绿单源：[21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)——本树是其 27 号面合批延续（不含 F0-2 asid 面，两形口径分谳见审计基线）。
- E2 序位单源：[31-E2重钉与F0-3F0-4复活七刀同批施工序核-符号面零耦合与钉板A语义自洽定谳.md](31-E2重钉与F0-3F0-4复活七刀同批施工序核-符号面零耦合与钉板A语义自洽定谳.md)——本件澄清其「任意序」适用域（回流合并树非 base 树）。
- E2 实施形单源：[30-12号E2重钉施工案隔离树实装验证-QEMU全标记通过与overhaul树BENCH标记冻结债.md](30-12号E2重钉施工案隔离树实装验证-QEMU全标记通过与overhaul树BENCH标记冻结债.md)——其补丁在本树不适用是正确行为（base 旧形在场）；BENCH 标记冻结债滚新刀由回流终验承接。
- 债面单源：[20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)（build-user E0407 HostHooks 债首登）。
- 证据可复现：worktree `git worktree add --detach <dir> 3f9ec497a`（本树停靠态即 WIP 形）；build = `CARGO_TARGET_DIR=<独立> cargo build -p berry-kernel`；台账 = `python3 tools/unsafe_audit.py`（1232 条）；真机 = `timeout 220 qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel $(bash tools/paths.sh kernel) < /dev/null`——`ktest done` 即 F0-3/F0-4 自检面全过标记。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百四十五节（随本批——base 现占至六百四十四，#604 实占）。
