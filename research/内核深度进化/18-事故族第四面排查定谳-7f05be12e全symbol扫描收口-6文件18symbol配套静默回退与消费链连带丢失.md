# 内核深度进化 18 号：事故族第四面排查定谳——7f05be12e 全 symbol 扫描收口（6 文件 18 symbol 配套静默回退＋消费链同笔连带丢失）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：17 号件下轮预案的「事故族第四面排查」执行件——对 fceb vs base tip 全仓 Rust pub-symbol 扫描，把 `7f05be12e` 同笔静默回退面一次清完：除已登记的 F0-1/F0-2/F0-3/F0-4 四面外，另抓到 **6 文件 18 个 pub symbol 同笔配套回退**（kern_lock klock_enter_irqsave/kern_acquire_irqsave_reentrant、arch lib.rs cache_poc_clean 双函数/reset_virtio_mmio_window、trap.rs iabort 双 symbol、tlbshoot test_inline_consume_once、mm.rs with_frames/frames_locked/alloc_lock 三锁助手、vmoledger contig/pin/mmio 六面），且**消费链同笔连带丢失**（heap/aspace/vmo/dpg/ktest 的调用点 base tip 全零、overhaul 侧 21 命中）——回退至今无编译红正因「供体与消费者同笔配套丢」，回流整取任一消费者文件即第六连编译红；13 号六步总序的 4 个整取件（vmar/aspace/dpg/ktest）全部踩中本面，第 0 步七点草案不扩点但施工判据升级为「整取消费者前先复活供体」。

## 审计基线（2026-11-20 03:2x 直核）

- base 线 = `origin/wip/sched-1.1@d25c75876`；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`。
- 扫描方法：`git diff --name-only fceb03ff3 origin/wip/sched-1.1`（171 个 .rs/Cargo.toml，剔除 docs/research/tests）逐文件提取两侧 pub symbol 集（fn/struct/enum/const/static/trait/type/macro_rules），`comm -23` 得 fceb 有 base 无全集 42 symbol；排除 13/15/17 号已登记面（vmar/aspace/asid/mmu/vmo/ktest）与 68b12fc41 探针族显式退役面（balface heartbeat_due/stuck_aged/stuck_scan_due、spin set_spin_bark、kern_lock set_kl_name_hook——base 侧仅注释残迹，perf 刀显式退役非回退）后，得未知面 24 symbol。

## 主发现：第四面 = 7f05be12e 同笔 6 文件 18 symbol 配套静默回退

- **排除法定谳流程**（逐 symbol 四问：base tip 命中数 / fceb 命中数 / overhaul 命中数 / `-S` 区间显式删除笔数）：
  - 18 symbol 全部满足 base=0、fceb≥1、overhaul≥1（与 fceb 同形）、`git log --full-history -S fceb03ff3..origin/wip/sched-1.1` 零命中——同签名定谳（与 13 号 F0-3/F0-4、15 号 F0-2、17 号 F0-1 三面判例同法）。
- **回退笔钉死**：first-parent 历史逐笔 symbol 计数扫描，18 symbol 计数 >0→0 跳变点全部落在 `7f05be12e`（merge: origin/wip 并流〔G0 刀 2 件随流〕）——与 F0-1/F0-2 同笔，事故族四面汇于一支。
- **owner merge 蔓延笔新发现**：`d25c75876`（owner pull 537 笔欠账 merge，b3a200533×27b35c111）——owner 本地分支 b3a200533 侧这些 symbol **仍全活**（with_frames=3 等），merge 无冲突自动取 origin 侧回退态，回退随 owner merge 蔓延成 base tip 现状。这解释了 17 号件「base tip 实测全零」的时点：27b35c111 时已回退。
- **丢面清单（base tip 实测全零，fceb/overhaul 同形全活）**：
  1. `arch/aarch64/src/kern_lock.rs`：`kern_acquire_irqsave_reentrant`（重入形取锁）+ `klock_enter_irqsave`（RAII 形入口）——13 号件 dpg.rs 段「F0-3 IrqSaveGuard 形」的底座函数即在此文件。
  2. `arch/aarch64/src/lib.rs`：`cache_poc_clean` / `cache_poc_clean_invalidate`（PoC 缓存维护双函数）+ `reset_virtio_mmio_window`（virtio-mmio 窗口重置）——17 号件⑥只恢复了 sync_icache_pou re-export，本三函数 re-export 同在 lib.rs 未被点名。
  3. `arch/aarch64/src/trap.rs`：`arm_expected_iabort` / `iabort_armed`（预期指令中止武装对）——ktest F0-3 断言面底座；iabort 消费在 ktest.rs（overhaul 侧 3 处）——ktest 并集解时若只恢复调用不恢复 trap.rs 定义即 E0425。
  4. `arch/aarch64/src/tlbshoot.rs`：`test_inline_consume_once`（跨核对飞测试助手）。
  5. `kernel/src/mm.rs`：`with_frames` / `frames_locked` / `alloc_lock`（帧分配器锁三助手）——aspace/vmo/heap 的 overhaul 形消费。
  6. `vmoledger/src/lib.rs`：`new_contiguous` / `is_contiguous` / `contig_drop_block` / `set_pinned` / `is_pinned` / `mmio_pa_range`（contig/pin/mmio 六面）——**13 号件 F0-3 只点了 vmo.rs/ktest 面，vmoledger 这个供体文件不在九文件清单**，本件补登。
- **消费链同笔连带丢失（第六连编译红预警）**：heap/aspace/vmo/dpg/ktest 五文件在 overhaul 侧共 21 处调用这些 symbol，base tip 全零——供体与消费者同笔配套丢使回退静默无编译红；13 号六步总序中第 1 步（vmar.rs 十段整取）与随行 aspace/dpg 整取、第 4 步（ktest 并集）四件全部踩中（vmo.rs 已在 F0-3 面登记），任一整取不复活供体即 E0425。**施工判据升级：整取任一消费者文件前，先按本件清单复活供体文件对应面**。
- **取形判据（第 0 步七点不扩点）**：六供体文件 fceb↔overhaul diff 零行（六文件 overhaul ≡ fceb 形）——「取 overhaul 形」与「取 fceb 形」等价，17 号七点草案操作不变，仅判据升级。
- **balface 四 symbol 排除依据**：`git log --full-history -S heartbeat_due fceb03ff3..origin/wip/sched-1.1 -- balface/src/lib.rs` 命中 `68b12fc41`（perf 探针族退役刀——hb/STUCK/PULL 三 static+扫描段+spin_bark 全族显式退役，commit message 明言「探针族退役」），base 侧 soak_crossed 命中 2 为注释残迹；显式退役演进非静默回退，不入第四面。

## 与其他文档的关系

- 事故族谱增笔：13 号（F0-3/F0-4）→ 15 号（F0-2）→ 17 号（F0-1 arch 面）→ **本件（第四面：6 文件 18 symbol 配套回退＋消费链连带丢）**——四面汇于 `7f05be12e` 同笔，族名来源 `3e323b8c9` D2 注记。
- 施工序单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——本件不改变其六步总序与文件取舍，仅补 vmoledger 供体文件登记与「先复活供体后整取消费者」判据。
- 第 0 步单源：[17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md](17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md)——七点草案不扩点；本面复活义务挂在六步总序各整取步的随刀面（第 1 步 vmar.rs/aspace/dpg 随刀、第 4 步 ktest 随刀、vmo.rs F0-3 面随刀）。
- 恒等区旁证：[16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md](16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md)（aspace 两形恒等）与本件六供体文件零 diff 同法——回流取形无三方分歧时操作等价。
- 证据可复现：回退笔判据 = `git show 7f05be12e~1:kernel/src/mm.rs | grep -c with_frames` = 3 vs `git show 7f05be12e:kernel/src/mm.rs` 同口径 = 0；vmoledger new_contiguous 同笔 4→0；`-S` 全区间零命中；owner 蔓延笔 = d25c75876 两亲 b3a200533（活）×27b35c111（死）取死。
- 沿革：`docs/阶段沿革.md` 六百二十一节（随本批）。
