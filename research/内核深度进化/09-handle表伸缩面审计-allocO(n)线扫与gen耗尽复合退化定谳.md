# 内核深度进化 09 号：handle 表伸缩面审计——alloc_slot O(n) 线扫与 gen 耗尽槽复合退化定谳

Type: research
Status: done
Updated: 2026-11-19

一句话说明：补货梯度「新审计面扫描」（boot/时基·handle 表伸缩·SMP 三候选）查实定谳——SMP 面 owner 文档群已占（母案 [SMP设计草案-v1.0.md](../../docs/SMP设计草案-v1.0.md) + S2-S5 实施草案 + [调度器SMP部署设计-v1.0.md](../../docs/调度器SMP部署设计-v1.0.md)，W3 不立项；boot/时基面 rtc.rs 84 行 + vdso.rs 62 行且 bootparams 已 bootpface 单源（W3-Q85），肉不足立项；**handle 表伸缩面立项**——本件实测三退化轴：①`alloc_slot` O(n) 前向线扫 × 14 个 syscall 插入点 × boot 插表 12+ 点（表龄越长扫越长）；②gen 14 位 ≈ 1.6 万次重用即耗尽，耗尽槽永久废弃且**不在 free list**（`Free { next_gen }` 只记代号不挂链）——复合恶化：线扫要跳过耗尽槽、耗尽槽永远占着 vec 实位；③64K 帽 = `slot_index_valid(65536)` 硬顶。ktest 双钉（:3707「最低空槽应被复用」+ :844「满装首收须最低位槽」）冻结分配序不变量——任何伸缩改造必须保最低空槽语义。三轴定级：不换数据结构（Vec 平铺 + 最低空槽序是 v1 唯一已被 ktest 钉板且 boot 插表可证的形态）；候选刀 = ①最低空槽游标 hint 单调化（amortized O(1)，reset 于 close/drain——语义等价最小刀）；②gen 域扩容**查实不可行**（gen 可用窗 = 32 − SLOT_BITS 16 − 预留位 2 恰 14 位已顶满，扩容必吞 §8.1 预留位或 u64 化破坏决策 #28——列 owner ABI 裁定件不入候选）——刀 A 列候选不实施，留 owner 裁定。

## 审计基线（2026-11-19 21:3x 直核）

- base 线 = `origin/wip/sched-1.1@eaa1b0538`；handle.rs 702 行（overhaul 线 730 行，alloc_slot 同形无分叉——回流面零冲突预判）。
- 数据点实测：syscall.rs `.insert(`/`insert_ref(` 14 处；boot 插表 main.rs 12+ 处（:974/:1231/:1239/:1274/:1299/:1333/:1385/:1437/:1473/:1484/:1500/:1530）；ktest test_handle（:3635）+ E3f 满装断言（:844）。
- SlotState = `Free { next_gen: u32 }` | `Used { gen, obj: NonNull<ObjRef>, rights: u64 }`——Free 变体**无链指针**，空槽发现全靠线扫。
- probe 面（hdlface/probe.rs，W3-Q91）已覆盖类型探测链折叠——本件不重审。

## 三退化轴实测

### 轴 ①：alloc_slot O(n) 前向线扫

`alloc_slot`（handle.rs :664）从槽 1 前向 `position()` 找首个 `Free` 且 `next_gen <= GEN_MASK` 的槽——O(表长)。每次 syscall 插入（channel_create/port 创建/事件对/duplicate 等 14 点）都付全额线扫。表前 100 槽全是 boot 期长寿对象（main.rs :974 起资源/rootfs/G0 引导双槽/事件/通道）——用户态 churn 永远扫过这段固定开销。**单次成本小（每槽一次 enum match），但表龄成本线性累积，且与轴 ② 复合**。

### 轴 ②：gen 14 位耗尽槽永久废弃——复合恶化主轴

gen 域 = 14 位（`GEN_MASK = 0x3FFF`），单槽 close/take/drain 各 +1，**≈ 1.6 万次重用即耗尽**；耗尽槽 `next_gen > GEN_MASK` 永久废弃（防回卷复活旧 handle——3b3 H3 安全设计，非债）。复合恶化在两层：其一，EL0 循环 create/close（如 send 收包随行 handle 高频收发）会把低位槽打满耗尽，`alloc_slot` 线扫要**跳过这些永久废槽**——扫描越长可用槽越少；其二，耗尽槽驻留 vec 实位（`Used→Free` 转换后无人回收），64K 帽下实际可用容量 < 名义 64K。**极限形**：低位槽全耗尽 + 高位连续尾插 → 每次插入线扫全表 O(64K)。

### 轴 ③：64K 硬帽

`slot_index_valid`（hdlface 单源）限槽位号 ≤ `SLOT_MASK`（16 位）——ABI §8.1 位域冻结，非债而是设计边界。但轴 ② 的废槽驻留使有效帽 < 64K，长寿命进程（server 形）先撞。

## 定谳与候选刀（不实施，owner 裁定）

1. **不换数据结构**：Vec 平铺 + 最低空槽分配序被 ktest 双钉（:3707/:844）+ boot 插表顺序可证——换 free-list/游标树/哈希都破坏「最低空槽」冻结语义，v1 收益不抵 ABI/测试面风险。overhaul 线同形（无分叉），回流不撞。
2. **候选刀 A（最低空槽游标 hint）**：`HandleTable` 加 `lowest_free_hint: usize`——insert 尾插时置新槽号（单调），close 置 `min(hint, slot)`，drain 归零；`alloc_slot` 从 hint 起扫。**语义等价**（仍最低空槽，只是起点前移）、amortized O(1)、零 ABI 面。风险点：close/drain/replace 路径的 hint 维护遗漏面（ktest 已有双钉可回归）。
3. **候选刀 B（gen 域扩容——查实不可行，改列 owner ABI 裁定件）**：gen 可用窗 = 32 − SLOT_BITS 16 − RESERVED_MASK 预留位 2 = **恰 14 位，已顶满**（hdlface/src/lib.rs 位域常量单源实况：`Handle = u32`、SLOT_BITS 16 占 [0,16)、RESERVED_MASK 0xC000_0000 钉死 bit30/31）。任何扩容只有两条路且都非「发行域单源改」：其一，吞 bit30/31 预留位——即便吞尽也只到 16 位 gen = 65536 周期（4 倍改善，非量级差），且触碰 ABI §8.1 预留面；其二，`Handle` 换 u64——推翻决策 #28「handle 表示 uint32_t」四十年外部合同（2026-10 已触发）。改 `GEN_BITS` 即改 handle 值域，非 `decode_gen` 掩码局部改。**定谳：v1 唯一零 ABI 伸缩路径 = 刀 A**；预留位启用/位域重构若要做，是 owner ABI 裁定件，代价即上述两条路。
4. **实施时点建议**：刀 A 小（≤ 50 行 kernel + hdlface 测试面），但当前 lane 无性能实证需求（无实测回归数据指认 alloc_slot 为瓶颈——02 号 IPC 审计「每 RPC 等待量子」瓶颈定谳在前），**列候选留 owner 裁定**，不抢回流窗口（overhaul 回流在即，syscall.rs 六段手术优先）；刀 B 已定谳不可行，无实施面。

## 与其他文档的关系

- 三候选查实定谳来源：上轮补货梯度（QUEUE 21:1x 轮「boot/时基·handle 表伸缩·SMP 预研新审计面扫描」）。
- handle 位域单源：[../../hdlface/src/lib.rs](../../hdlface/src/lib.rs)（W3-Q8 钉板——gen 窗 14 位顶满的常量实况在此，刀 B 不可行定性依据）。
- ktest 不变量钉板：kernel/src/ktest.rs :3707（最低空槽复用）/:844（E3f 满装最低位槽）——候选刀 A 的回归面。
- gen 耗尽安全设计（防回卷复活）：kernel/src/handle.rs `take_gen` 注释（3b3 审查 H3）——本件轴 ② 的「非债」定性依据。
- 沿革：`docs/阶段沿革.md` 六百零九节（随本批）。
