# 内核深度进化 35 号：33＋34 两树全 F0 面合树 QEMU 真机终验——F0-1/F0-2/F0-3/F0-4 一树首闭合（35 号件）

一句话说明：33 号验了第 0 步＋27 号七刀（kern_lock/tlbshoot/mm/vmo/abi_gen/ktest/memface/mmu F0-1·F0-4 面）真机、34 号验了 F0-2 消费面（asid.rs 整取/aspace 取号窗/syscall 消费窗/vmar drop 归还/mmu F0-2 面）真机——但**两树合体后的全 F0 面一树真机回归从未跑过**（F0-1/F0-4 的 mmu 面〔CTR_EL0 惰性缓存/VirtIO-MMIO Device 直映块〕与 F0-2 的 ACTIVE_CPU_ASID 记账面在同一 mmu.rs 里共存、syscall.rs 里 F0-4 三 options 位与 F0-2 取号窗同文件交汇、ktest 里 f0_3/f0_4 断言与 vmo_create 合法位集断言 2→8 联动）。本件闭合这个洞：33 树 22 文件与 34 树十文件的并集（27 文件）base tip `b7155dd05` 干净贴入，`cargo build -p berry-kernel` EXIT=0（27 警全 base 既有）、lockdep 双形 0 error、台账随源重产 1229→1233 `--check` ok（+4 = mmu.rs 合树新增 unsafe 面〔CTR_CACHE 读点/VIRTIO 窗直映〕）、wg 91、docs-guard/abi-check PASS——**QEMU 真机 `ktest done`（:120）＋ktest done 前 58 标记全到（含 BENCH ok n=10、`ASPACE ok (asid=16b)`）＋expected D-abort 恢复链 10 处正常＋零 panic 零 FAIL**；host 挂测面 mm 56＋asidface 5＋vmoledger 28 全绿；边界定谳沿 33/34 号（build-user 红＝20 号 E0407 债同指纹、对照红属合并树域 19/21 号已证）。

## 1. 背景与验证留白

- 33 号：第 0 步＋27 号七刀合树真机 ktest done——面 = kern_lock 双 guard/tlbshoot 探针/mm allocated_pages/vmo contig·global/abi_gen 常量族（NR_VMO_CACHE_OP/VMO_OPT_CONTIGUOUS/PINNED 等）/ktest f0_3·f0_4/memface 本体/mmu F0-1·F0-4。
- 34 号：F0-2 消费面真机 ktest done——面 = asidface crate/asid.rs 分代位图整取/aspace try_alloc+free/vmar drop 归还/mmu F0-2（ACTIVE_CPU_ASID/tcr_as_config/asid_shift=48）。
- 留白：两件互为单面验证；合树后 mmu.rs 同时携带 F0-1 CTR_CACHE/F0-2 ACTIVE_CPU_ASID/F0-4 VIRTIO 直映三族 unsafe 面、syscall.rs 同函数 `sys_vmo_create` 膨胀四分支、ktest vmo_create 未知位断言 2→8（bit1 CONTIGUOUS＋bit2 PINNED 激活后合法集 = DEMAND|CONTIGUOUS|PINNED = 7，非法 = 8）——联动回归从未真机验过。
- 本件验证口径：**全 F0 面（F0-1/F0-2/F0-3/F0-4）一树合体后的 QEMU 真机回归**——回流主批的完整 F0 预演。

## 2. 施工资序（27 文件合树）

基座 = base tip `b7155dd05`（34 号件已合入；含 33 号七刀面）。两树合体 = 34 号十文件增量（w3-34 证据树 diff 8 文件平移＋memface 本体 94 行）叠在 33 号 22 文件面上（33 号已入 base，b7155dd05 天然携带）。

1. **asidface crate 三件**（Cargo.toml/src 220 行/tests 97 行）＋根 members＋arch dep＋mm dep——34 号原笔。
2. **mm/src/asid.rs 整取**：`AsidState` 分代位图形（try_alloc(pinned)/free/generation/force_near_roll）。
3. **kernel/src/aspace.rs**：try_alloc_asid（pinned 钉死面）＋alloc_asid 便捷形＋free_asid。
4. **kernel/src/syscall.rs**：sys_process_create 耗尽 -ENOMEM 臂＋free_asid 归还；F0-4 sys_vmo_cache_op（0x1C）＋vmo_create 四分支（demand/contiguous/pinned/plain）＋合法位集扩 CONTIGUOUS|PINNED。
5. **kernel/src/vmo.rs**：create_pinned/create_contiguous_global（伙伴阶分配＋DC ZVA 清零＋退回臂）/cache_op（copy_gate＋walk_copy 注入 cache_poc_clean[invalidate]）。
6. **kernel/src/vmar.rs**：Drop 归还 free_asid。
7. **arch/aarch64/src/mmu.rs**：F0-2 面（ACTIVE_CPU_ASID[2]/active_cpu_asids/asid_bits→tcr_as_config/asid_shift const 48）＋F0-1 面（CTR_CACHE 惰性缓存）＋F0-4 面（VIRTIO_MMIO0_PA Device 直映块——MMIO VMO Drop 的 fatal D-abort 防御）。
8. **kernel/src/ktest.rs**：f0_3/f0_4 定义体＋run() 尾调用＋vmo_create 未知位断言 2→8 联动。
9. **memface/vmoledger 扩面**：memface 本体（cache_line_span/ctr_dmin_line/ctr_pou_plan/IcachePolicy）；vmoledger +81 行（set_pinned/new_contiguous/copy_gate 复用面）。
10. **ABI 五面单源**：abi_gen.rs 两端（42 号在册）/abi.toml（nr_total 42·ceiling 46）/abi.syz/ABI 设计文档 0x1C 行＋v1 计数 41→42。

## 3. 六门实测

| 门 | 结果 | 注 |
| --- | --- | --- |
| cargo build -p berry-kernel | EXIT=0 | 27 警全 base 既有对照 |
| lockdep 双形（--features lockdep） | 0 error | 单元计数不漂 |
| unsafe 台账（重产 + --check） | 1233 条 ok | 1229→1233，+4 = 合树 mmu.rs 新增面（CTR_CACHE/VIRTIO 窗） |
| writable-globals | ok（91 entries） | 不变 |
| docs-guard | PASS（0 警） | G10 201=201 |
| abi-check | PASS | 0x1C enforced_since WARN = 27 号刀④义务面（在案） |

## 4. QEMU 真机门（本件主验面）

命令：`timeout 220 qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel $(bash tools/paths.sh kernel) < /dev/null`（内核相直跑，33/34 号同款；内核 ELF 由隔离 CARGO_TARGET_DIR 产出）。

- **`ktest done` 到达（:120）**——全 F0 面合树真机首闭合；f0_3/f0_4 纯断言面（失败即 panic 中断，done 到达律＝自检面全过）。
- **ktest done 前 58 个标记全 ok**：KVA/GIC/FRAME/HEAP/SYSCALL/PING/FPING/**ASPACE ok (asid=16b)**（F0-2 宽度探测面）/VMO/HANDLE/POLICY/调度 K 族 25 面（PV3/PBS/PP/SHD/DV/POLS-R/YIELD/PLACEMENT/RING2/RING3/PI/PILAT/INTENT/BAND/FAST/FREEZE/DL/BL/CMP/SUS-NEG/FRZCYC/WAKEP/BL-OVF/ATTR-NEG/PERF/ATTR/WSRC/DTB/PWRDEG）/EM/UTIL/OVERUTIL/HDR/HDRWAKE/INTEG-K/AM2B-K/SYSVER/BANDM4/EAS/LOADGEN/EASPROD/SMPLZ/BAND-SMP/EASPROD-SMP/SLACK-SMP/THRYLD/STEALBND/CBOXK/DENS——**BENCH ok n=10 也到**（churn 相在 220s 窗内跑完，比 33 号 29/30 更全）。
- **expected D-abort 恢复链 10 处正常**（E2 旧形 RO 写/unmap 读/切分中页 fault 族）——F0 面复活未搅动 fault 恢复语义。
- 无 panic/无 FAIL/无 allocation failure。

## 5. host 挂测面

- `cargo test -p berry-mm --target aarch64-apple-darwin`：56 passed（asid.rs 挂测刷新形）。
- `cargo test -p berry-asidface --target aarch64-apple-darwin`：5 passed。
- `cargo test -p berry-vmoledger --target aarch64-apple-darwin`：28 passed（含 contig/pinned 台账新面）。

## 6. 边界定谳

1. **build-user 红 = 20 号在案 E0407 HostHooks 债**：berry-launcher `render_hold` E0407 同指纹（33/34 号同款记录）；本树代码面（syscall/vmar/aspace/mm/mmu/asidface/vmoledger）零 launcher 触点，非本刀引入。内核相直跑验面已覆盖；smoke 完整相（rootfs）不可跑如实记录。
2. **对照红补验豁免依据**：19/21 号已证合并树域对照红，本树为 base 增量贴入形，无对照红义务。
3. **⑥债**：#606 已闭偿（user/Cargo.toml members 已补 berry-hal——base 前进 b7155dd05 后复核在树）。

## 7. 与既有文档的关系

- 七刀真机单源：[33-第0步与27号七刀合树隔离实装QEMU真机终验-ktestF0-3F0-4自检面真机首闭合.md](33-第0步与27号七刀合树隔离实装QEMU真机终验-ktestF0-3F0-4自检面真机首闭合.md)——本件为其与 F0-2 面的合树终验。
- F0-2 真机单源：[34-F0-2asid复活面隔离实装QEMU真机终验-分代位图分配器真机首闭合.md](34-F0-2asid复活面隔离实装QEMU真机终验-分代位图分配器真机首闭合.md)——本件为其实树合体续验件。
- 施工图单源：[17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md](17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md)——第 0 步全 F0 面的真机实施总验。
- 复活面定谳单源：[27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md](27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md)。
- 证据可复现：27 文件全 diff 随批入册（`35-f0全family合树.patch`）；build/台账/QEMU 命令同 33/34 号。

## 8. 结论

33＋34 两树的合树留白至此闭合：全 F0 面（F0-1 CTR_EL0 PoU/F0-2 分代位图 ASID/F0-3 锁守卫 RAII 自检/F0-4 DMA 连续块＋缓存维护）27 文件一树合体，六门绿，QEMU 真机 ktest done＋58 标记＋BENCH ok＋D-abort 恢复链正常，host 挂测 89 测全绿。**回流的完整 F0 面真机就绪，可随回流主批合龙**——回流施工单 F0 面四刀（第 0 步＋七刀＋F0-2 十文件＋本合树笔）全部真机验证完毕，剩余回流义务面 = E2 重钉（30/31 号）与逐文件段处置（22-29 号施工卡）。
