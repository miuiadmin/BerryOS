# 内核深度进化（2026-11-17）

> 四路并行审计（内存管理/IPC 快路径/syscall 分派/调度热路径），找 Top-5 优化机会。

| # | 文件 | 主题 |
|---|---|---|
| 01 | `01-内存管理优化审计.md` | 分配器/VMO 页表遍历/退出回收——Top-5(含 zero_frame DC ZVA 快赢) |
| 01a | `01-机会3立项前置研究-进程退出回收双线对撞审计.md` | 机会 3 立项前研究：base-overhaul 线 F1-2 已承接，W3 不立项——01 号收官账 |
| 02 | `02-IPC快路径审计.md` | recv 10x walk/PI 重推门/消息环形化——Top-5 |
| 03 | `03-syscall分派审计.md` | 诊断残留退役/KERN_LOCK 归零/SVC 包络测量——Top-5 |
| 04 | `04-调度热路径审计.md` | preempt 门控/诊断退役/per-CPU 原子——Top-5 |
| 05 | `05-base-overhaul回流接缝预研-W3面资产对撞审计.md` | base-overhaul 回流时 W3 face crate 群对撞四态定级＋回流义务清单 |
| 06 | `06-recv_n重放形态预研-ChannelShared接缝审计.md` | 回流义务 3 深化：消费点六接缝施工图 + disarm 双重释放风险钉板 |
| 07 | `07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md` | 回流义务 2 深化：三查判序两形 200k 样本恒等对拍＋退役/保留/迁移裁定 |
| 08 | `08-syscall逐段对拍清单预备-回流六冲突段施工图.md` | 回流义务 5 深化：syscall.rs 回流六冲突段施工图（3 overhaul 权威 + 2 base 权威 + 1 组合重放）＋KLOG 窗缘/权利域两域裁定 |
| 09 | `09-handle表伸缩面审计-allocO(n)线扫与gen耗尽复合退化定谳.md` | 新审计面三候选查实（SMP/boot 不立项定谳）＋handle 表三退化轴：线扫×churn×gen 耗尽复合——候选刀 A 游标 hint/B gen 扩容，留 owner 裁定 |
| 10 | `10-bench测量面内联段查实与回流接缝补遗.md` | bench.rs 内联段不立项定谳（测量面物理臂）＋05 号回流审计补遗：bench.rs 分叉 177 行但自动合并零冲突、依赖闭包全活——base 权威正和件零手术 |
| 11 | `11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md` | 回流义务 6 深化：vmarface 入册八触点对拍——六零手术＋两手工点＋overhaul 白名单漏登债代偿 |
| 12 | `12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md` | 回流义务 4 深化：E2 基础三面钉逐字同文重钉＋半提交零残留/重 map 零树残留/protect 切分三增量钉板施工案 |
| 13 | `13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md` | 回流义务 5 补遗：merge-tree 全仓 10 冲突文件 38 段总账——ktest 调用序列并集＋F0-3/F0-4 base 线静默丢失复活义务（主发现）；vmar 十段/aspace/dpg 全取 overhaul；channel 五段=06/07 实弹；零手工四文件 |
| 14 | `14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md` | channel.rs 段 2 裁定项数据补强：Q95 环形 vs F1-3 Vec 存储 host 微基线（满队 2.34x/回滚 7.6x/k=1 持平）＋六消费点改接清单＋三路裁定选项（推荐路 C：骨架优先、环形资产保留、收益决策推迟到真机 profile） |
| 15 | `15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md` | F0-2 全家面 base 线静默回退定谳：谱系实锤（切换点 7f05be12e，w1 侧枝三跳接力回灌）＋合并树隐藏编译红四连（try_alloc/free_asid/active_cpu_asids/挂测行孤儿）＋11 号件「不复活死行」裁定修正＋13 号件施工序插第 0 步（F0-2 复活随刀） |
| 16 | `16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md` | try_alloc_asid 两形语义对拍（Q110 游标面消亡清单件）：ASID 分配面四函数 token 级恒等（零手术恒等区）＋destroy 路径语义差随第 1 步整取自动承接＋asidface 双代数并存澄清＋Q110 游标面消亡清单闭合 |
| 17 | `17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md` | 13 号施工序第 0 步实施化：F0-1/F0-2 复活随刀逐步 diff 草案（七触点逐 hunk 可施工，15 号六点扩为七点）＋主发现事故族第三面——F0-1 arch 面（CTR_EL0 PoU 缓存一致性）同点 7f05be12e 静默回退（mmu.rs 5→0/arch dep 1→0/lib.rs re-export 摘）＋第五连编译红预警（vmar.rs 整取后 hal::sync_icache_pou E0425）＋ktest G/H 验收面静默丢失登记＋R490 五笔重放义务 |
| 18 | `18-事故族第四面排查定谳-7f05be12e全symbol扫描收口-6文件18symbol配套静默回退与消费链连带丢失.md` | 17 号件预案首项执行：fceb vs base tip 全仓 Rust pub-symbol 扫描——事故族第四面定谳，7f05be12e 同笔 6 文件 18 symbol 配套静默回退（kern_lock/arch lib/trap/tlbshoot/mm/vmoledger）＋消费链同笔连带丢失（heap/aspace/vmo/dpg/ktest 21 处 base 全零）＝第六连编译红预警；owner merge d25c75876 蔓延笔实证；排除 68b12fc41 探针族显式退役 6 symbol；施工判据升级：整取消费者前先复活供体，第 0 步七点不扩点（六供体 fceb≡overhaul 取形等价） |
