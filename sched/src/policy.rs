//! AI 策略注入——**三槽 RCU 式原子换表基础设施**（3-4c 留桩的机制面升级；
//! `AI调度设计.md` §2/§9、`路线图.md` Phase 3 出口增项）。
//!
//! 形态演进：策略表不再持 `&'static [Entry]` 热路径线性扫，而是**快照化**——
//! [`bind`] 把 `&[Entry]` 语义映射进 [`Snap`]（哈希桶 + 定长槽），经 [`publish`]
//! 原子换入三槽轮转区；决策路径（[`apply`]/[`weight_of`]）经 [`ACTIVE`]
//! Acquire 载入取**一份稳定快照**指针，O(1) 开放寻址查表（≤[`PROBE_MAX`] 步
//! 上界）。这是 v3 `policy_bind`（0x83）换表协议的机制底座**提前落地**——
//! 「事务化原子换表快照」半件已在本模块，共享环 + 键域扩展半件随 v3 协议落。
//!
//! 空态等价性不变（§2「AI 不在线时表为空/默认，等价于纯启发式」）：未绑
//! （[`ACTIVE`] = [`SENTINEL_EMPTY`] 哨兵 → [`EMPTY_SNAP`），或已绑空表
//! （`count == 0`）→ [`apply`] 两次分支即返、**零写入**，与「接口缺席」逐位
//! 等价（ktest `POLICY ok` 断言，非口号）。
//!
//! 动作空间四类封顶（§2 战略裁决）：①放置 hint；②频率/热限 hint（**走 PM
//! 服务**、不经调度路径——不在表里）；③包络内权重微调；④EPP 二值。**没有
//! 第五类**（尤其没有「直接改 RT 优先级/创建/杀任务」）。
//!
//! 并发纪律（SMP 三件套①原子化 ②hazard-lite ③写者锁俱落）：
//! [`apply`]/[`weight_of`] 对**表**只读——IRQ 屏蔽的决策路径调用，每次
//! 调用取一次快照指针、窗内不换（RCU 读临界区形态；apply 另递 divergence
//! 对账计数器——纯原子统计，非表写、非 TCB 写，见文件尾「Shadow
//! divergence 对账」节）；写点经 [`POLICY_LOCK`] 写者自旋锁串行化（③——
//! bind/bind_gen/bind_gen_shadow/promote/demote 入口持锁，gen 检查+写入
//! 等复合临界区原子化；[`unbind`] 仍 boot/ktest 清理专用单写者裸存）。
//! 槽一经发布内容不再改写——②起写者只写**非活跃且无 hazard**槽（读者进窗向
//! 本核 [`HAZARD`] 位发布在读槽号并验证 ACTIVE 未翻转，写者 [`publish`]
//! 扫描避让），他核在用槽不被覆写；读窗串行（现状屏蔽窗/KL）形态下两核
//! hazard 收敛同槽、阻塞集 ≤2 恒有空闲——单核/现状与旧固定轮转等价
//! （锁恒立即获取，零行为变化）。
//!
//! 语义映射的边界（写者是受信内核侧，拒收面在此收口）：
//! - **key == 0 保留**：空槽哨兵键，注入条目带 0 键直接丢弃（`proc_tag = 0`
//!   的内核任务本就不可注入）；
//! - **重复键 first-wins**：与旧线性扫 `find` 语义一致（先到先得）；
//! - **溢出拒收**：条目数超 [`TBL_MAX`] 或探测窗（[`PROBE_MAX`] 步）满 →
//!   丢弃该条——O(1) 查表上界的代价面；
//! - **权重三点夹紧**：build 时夹 + [`apply`]/[`weight_of`] 消费点再夹
//!   （±[`WEIGHT_MAX`] 单调收窄，F9②——夹取是唯一方向，无提权路径）。

use crate::task::{EppHint, Task};
use berry_hal_aarch64 as hal;
use core::sync::atomic::{fence, AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

// ---- 判定面单源（W3-Q20）：常量族与查表/插槽/EPP 打包数学已迁
// `berry-polface`（host 可测钉板——散列锚点/探测窗不变式/first-wins/
// 窗满拒收/打包往返 13 测）；此处 re-export 后零第二实现，本文件只剩
// 存储机械（SNAPS/SHD 三槽双槽区 + 原子发布/hazard-lite/写者锁）。----
pub use berry_polface::{
    bucket_of, epp_pack, epp_unpack, lookup_slot_idx, InsertOut, KEY_EMPTY,
    BUCKETS, EPP_HAS, EPP_VAL, PROBE_MAX, Slot, TBL_MAX, WEIGHT_MAX,
};

/// [`ACTIVE`] 的哨兵值：三槽 0..=2 之外的 3 = OFF/回退态（指向 [`EMPTY_SNAP`]）。
const SENTINEL_EMPTY: u32 = 3;

/// 单条策略指令（v1 词汇表：③权重 + ④EPP + ①放置记录）。
#[derive(Debug, Clone, Copy)]
pub struct Directive {
    /// ① 放置 hint（目标核/簇；0 = 无意见）。v1 单核不消费，仅落字段。
    pub placement: u8,
    /// ③ 包络内权重微调（应用时夹紧 ±[`WEIGHT_MAX`]，写入 `prio_effective`）。
    pub weight: i32,
    /// ④ EPP 二值（`None` = 不覆盖）。
    pub epp: Option<EppHint>,
}

/// 表条目：`task_key` 命中的任务应用 `directive`。
///
/// v1 key = [`Task::proc_tag`]（进程标签；内核任务共 0——已有唯一稳定载体；
/// **0 键被空槽哨兵保留、注入即弃**）；v3 真 ABI 的 per-thread 键随
/// `policy_bind` 协议定（届时换键源，接口不变）。
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub task_key: u64,
    pub directive: Directive,
}

// ---- 快照表示（#[repr(C)]：槽内 key 落偏移 0，可按位审计）----
// 槽值形单源 = `berry_polface::Slot`（W3-Q20 外迁）；本文件以别名消费，
// 既有代码路径零改动。

/// 快照槽：一条指令的扁平表示（16 字节）。`key == 0` = 空槽。
type SnapSlot = berry_polface::Slot;

/// 策略表快照：发布后**内容不可变**（写者只写非活跃槽——RCU 式换表）。
#[repr(C)]
struct Snap {
    /// 世代计数（每次 [`bind`] 递增；v3 遥测/换表协议的世代词汇先落地）。
    gen: u64,
    /// 实际入槽数（0 = 空表——[`apply`]/[`weight_of`] 的快路径分支位）。
    count: u32,
    _pad: u32,
    slots: [SnapSlot; BUCKETS],
}

impl Snap {
    /// 全零空快照（const fn——静态初始化 + build 基底两用）。
    const fn empty() -> Snap {
        Snap {
            gen: 0,
            count: 0,
            _pad: 0,
            slots: [SnapSlot::EMPTY; BUCKETS],
        }
    }
}

// ---- 三槽轮转 + 哨兵空表 ----

/// 三槽轮转区：写者写**非活跃且无 hazard**槽（三件套②）、翻转 [`ACTIVE`]
/// 发布——读者经指针取整表。
/// `static mut`（写面——`&raw mut` 取裸指针，不立引用；`static` 非 mut 拒
/// 可变裸指针借用，E0596——同 `class.rs` LAST_VCT 纪律）。
static mut SNAPS: [Snap; 3] = [const { Snap::empty() }; 3];
/// 哨兵空表——OFF/回退态（`ACTIVE == SENTINEL_EMPTY` 时指向这里）。
static EMPTY_SNAP: Snap = Snap::empty();
/// 活跃槽号（0..=2）或 [`SENTINEL_EMPTY`]。发布序：槽写完 → fence(Release)
/// → 本位 store(Release)；读取 Acquire——读者看见槽号即看见槽内容。
static ACTIVE: AtomicU32 = AtomicU32::new(SENTINEL_EMPTY);
/// 世代计数器（单写者递增，非通信值——Relaxed 足够）。
static GEN_NEXT: AtomicU64 = AtomicU64::new(1);

// ---- hazard-lite 槽回收（SMP 安全三件套②）----
//
// 防写者覆写他核在用槽：读者进窗先向**本核固定位**发布在读槽号，写者
// [`publish`] 扫描 {ACTIVE} ∪ HAZARD 避让。比真 hazard pointer 简在结构：
// 固定位（无链表、无动态分配）+ 无回收回调（避让判定内联在 publish）。
// 读者出窗**不清**（简化）：读窗在 IRQ 屏蔽窗内有限，下次进窗覆盖旧值
// 自然释放；单核形态 HAZARD[1] 恒 [`SLOT_NONE`] → 写者扫描恒让过。

/// hazard 位核数（NCPUS=2 冻结配置，同 `telemetry.rs` 口径；拓扑扩核时
/// 本数组与 [`publish`] 扫描同扩）。
const HAZARD_CPUS: usize = 2;
/// [`HAZARD`] 的未在读哨兵：0xFF，远离 ACTIVE 值域（0..=3）——永不与任何
/// 槽号/哨兵相等，写者扫描视同「该核未在读」。
const SLOT_NONE: u32 = 0xFF;
/// 每核在读槽号发布位：值 = 本核当前/最近读窗的槽号。ACTIVE 哨兵 3 原样
/// 落位——EMPTY_SNAP 不可变无需保护，且 ∉0..=2 对写者扫描惰性（顺带 =
/// 读者回落哨兵态时自然释放旧槽号）。
static HAZARD: [AtomicU32; HAZARD_CPUS] = [const { AtomicU32::new(SLOT_NONE) }; HAZARD_CPUS];

// ---- 策略写者自旋锁（SMP 安全三件套③）----
//
// 串行化所有策略表写者（bind/bind_gen/bind_gen_shadow/promote/demote）：
// bind_gen 的「gen 检查 + 写入」、promote 的「shadow 读 + 4KiB 拷贝 +
// publish」等复合写临界区须原子——两核写者交错会造成 gen 倒灌/旧表覆盖
// 新表（世代单调性被竞态穿透）。比 KL 轻量：无递归、无中断区分（写点
// 全在 SVC 上下文；决策路径 apply/weight_of 只读不加锁——读者由②
// hazard-lite + 已发布槽不可变保护，与写者锁无交集故无死锁面）。
// 锁序：在冻结锁序中钉「表/KL」档——禁嵌 RQ/SLEEPQ 窗内获取（持 RQ/
// SLEEPQ 者不得再进写者入口；POLICY_LOCK 窗内亦不得反向取队列锁）。
// 内部写辅助（build_snap/publish）不自行加锁——由公共写者入口统一持有
// 后调用；错误路径（early return）同解锁，锁不泄漏。
// 单核：CAS 首试即成（无争用）→ 零行为变化。
static POLICY_LOCK: AtomicBool = AtomicBool::new(false);

/// 取写者锁（三件套③）：CAS 自旋。失败侧 Relaxed（仅重试判定，无同步
/// 语义）；成功侧 Acquire——临界区内写（槽写/ACTIVE 翻转）不下沉到锁外。
fn policy_lock() {
    while POLICY_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

/// 放写者锁：Release——临界区写先于锁释放对他核可见（解锁即发布）。
fn policy_unlock() {
    POLICY_LOCK.store(false, Ordering::Release);
}

// bucket_of 单源 = `berry_polface::bucket_of`（上方 re-export 位）；散列
// 锚点指纹钉板在 polface host 测（改乘数/移位量必红）。

/// 发布新快照（写者协议，三件套②）：扫描选**非活跃且无 hazard** 的槽
/// 写入 → fence(Release) → ACTIVE.store(Release) 翻转。
///
/// 槽选择不再固定 `(cur+1)%3` 轮转，改避让扫描——跳过①当前活跃槽（新
/// 读者持续经 ACTIVE 进入）②两核 [`HAZARD`] 位（他核在读/最近读窗的槽，
/// 未确认退出前不得复用）。hazard 位 Acquire 载入与读者「store(Release)
/// + fence」发布序配对：读者先发 hazard 后读数据、写者先读 hazard 后写
/// 槽。**「3 槽 2 核恒有空闲」**：读窗串行（现状屏蔽窗/KL）形态下两核
/// hazard 收敛同槽号 → 阻塞集 ≤2；若满 3 即 expect 响——响 = 协议违约
/// 当场抓住（wedge 优于静默跨核腐坏，同 `smp.rs` park 槽纪律）。写者经
/// [`POLICY_LOCK`] 全串行（③）无需 CAS；ACTIVE 入口载 Relaxed 即精确
/// （锁内单写者自见 + 写点全串行）。
fn publish(new: Snap) {
    let active_idx = ACTIVE.load(Ordering::Relaxed);
    let mut write_idx = None;
    for i in 0..3u32 {
        if i == active_idx {
            continue; // 活跃槽：新读者持续进入，不可覆写
        }
        if HAZARD[0].load(Ordering::Acquire) == i || HAZARD[1].load(Ordering::Acquire) == i {
            continue; // 他核在用（不清简化下含最近用）槽：跳过
        }
        write_idx = Some(i);
        break;
    }
    let w = write_idx.expect("3 槽 2 核恒有空闲：hazard+active 满覆盖 = 协议违约");
    // SAFETY: 单写者独占写**非活跃且无 hazard**槽——在窗读者只经 ACTIVE
    // 指针到槽且在读槽号已发布 HAZARD（写者扫描已避让），写窗内无别名
    // 引用。static 存储期 = 整个程序运行期，ptr::write 不越界（槽号 < 3
    // 编译期可证）。
    unsafe { core::ptr::write(&raw mut SNAPS[w as usize], new) };
    fence(Ordering::Release);
    ACTIVE.store(w, Ordering::Release);
}

/// 取活跃快照指针（读者协议，三件套②）：Acquire 载入槽号 → 发布本核
/// hazard（store(Release)）→ fence(Acquire) → **验证 ACTIVE 未在发布窗内
/// 翻转**——翻了则刚发布的槽号可能在发布生效前已被复用（load→store 窗，
/// 写者背靠背 publish 可回收该槽），换新槽号重发再验。这是经典 hazard
/// pointer 的验证环；lite 在结构（固定位/无回收回调），不在省略验证。
/// 验证 Acquire 载入把后续快照数据读定在 hazard 发布之后（AArch64 上
/// STLR→LDAR 的 RCsc 全序 + fence 挡编译期重排）。单核/读窗串行（现状）
/// 形态下验证首次即过，零行为变化。返回裸指针——发布后槽内容不可变，
/// 调用点一次解引用挂 SAFETY（读临界区内快照稳定）。
fn active_snap() -> *const Snap {
    // 拓扑外防御收敛（同 telemetry.rs 口径）：cpu_id 只应 0..=1。
    let cpu = hal::smp::cpu_id().min(HAZARD_CPUS - 1);
    let mut idx = ACTIVE.load(Ordering::Acquire);
    loop {
        // 发布：本核在读 idx（哨兵 3 原样落位——对写者扫描惰性）。
        HAZARD[cpu].store(idx, Ordering::Release);
        fence(Ordering::Acquire); // 发布先于验证读/数据读
        // 验证：hazard 生效前 ACTIVE 未翻转。翻转 ⇒ 换当前槽号重发；
        // 单写者下 ACTIVE 静默期必至，环有限步收敛。
        let recheck = ACTIVE.load(Ordering::Acquire);
        if recheck == idx {
            break;
        }
        idx = recheck;
    }
    // SAFETY: `static mut` 取 `&raw const` 裸指针（不立引用——E0133 解除的
    // 就是这一层）；解引用的读窗纪律在调用点（apply/weight_of 各挂 SAFETY）。
    unsafe {
        match idx {
            0 => &raw const SNAPS[0],
            1 => &raw const SNAPS[1],
            2 => &raw const SNAPS[2],
            _ => &raw const EMPTY_SNAP, // OFF/回退态
        }
    }
}

/// O(1) 查表：开放寻址线性探测，≤[`PROBE_MAX`] 步；`key == 0`（空槽哨兵）
/// 早退。**空槽早退可靠性**：插入与查表走同一探测序列、插入落**首个**空槽
/// → 键在槽 i 当且仅当其探测序列 0..i 全被占——查表先遇空槽即可断缺席。
fn lookup_o1(snap: &Snap, key: u64) -> Option<&SnapSlot> {
    // 判定律单源 `berry_polface::lookup_slot_idx`（W3-Q20 外迁——探测环
    // 唯一实现），此处零拷贝壳：切片借入、下标回投引用（生命周期挂
    // snap，消费点 apply/weight_of 的 &SnapSlot 面不变）。
    berry_polface::lookup_slot_idx(&snap.slots, key).map(|i| &snap.slots[i])
}

/// 快照插槽（壳——判定律单源 `berry_polface::insert_slot`，W3-Q20 外迁）：
/// 探测窗内首个空槽写入；重复键 first-wins；窗满拒收。消费侧按
/// [`InsertOut`] 三态入账（越包络夹取/窗满拒收的原子计数归本模块——纯面
/// 无存储）。返回是否真插入。
fn snap_insert(snap: &mut Snap, key: u64, d: &Directive) -> bool {
    // 判定与写入单源；计数按三态分支（与旧内联实现逐语义对齐）。
    let out = berry_polface::insert_slot(
        &mut snap.slots, key, d.weight, d.placement, d.epp,
    );
    match out {
        InsertOut::Inserted { weight } => {
            if weight != d.weight {
                CLAMP_WEIGHT.fetch_add(1, Ordering::Relaxed); // 越包络被夹入账
            }
            true
        }
        InsertOut::Duplicate => false, // 重复键：先到先得（无计数面）
        InsertOut::ProbeFull => {
            REJECT_PROBE_FULL.fetch_add(1, Ordering::Relaxed); // 窗满拒收入账
            false
        }
    }
}

/// 从 `&[Entry]` 构建快照：TBL_MAX 截尾、0 键丢弃、重复/溢出拒收（计数只
/// 记真入槽者）。build 后即不可变——发布后无人再改写。
fn build_snap(tbl: &[Entry], gen: u64) -> Snap {
    let mut snap = Snap::empty();
    let mut n: u32 = 0;
    // 拒收入账：超载截尾（take 前先记——被截掉的每条都是注入方给多的）。
    REJECT_OVERFLOW.fetch_add(tbl.len().saturating_sub(TBL_MAX) as u64, Ordering::Relaxed);
    for e in tbl.iter().take(TBL_MAX) {
        if e.task_key == KEY_EMPTY {
            REJECT_KEY0.fetch_add(1, Ordering::Relaxed); // 哨兵键丢弃入账
            continue; // 哨兵键保留
        }
        if snap_insert(&mut snap, e.task_key, &e.directive) {
            n += 1;
        }
    }
    snap.gen = gen;
    snap.count = n;
    snap
}

/// 绑定表（v1 仅内核侧 ktest/boot 期调用；v3 由 `policy_bind` 经共享环换表）。
/// 语义映射：`&[Entry]` → [`Snap`] 快照 + [`publish`] 原子换入。写者入口
/// （③）：[`POLICY_LOCK`] 全程持锁——gen 自增与 publish 同锁内原子。
pub fn bind(tbl: &'static [Entry]) {
    policy_lock();
    let gen = GEN_NEXT.fetch_add(1, Ordering::Relaxed); // 锁内单写者，非通信值
    publish(build_snap(tbl, gen));
    policy_unlock();
}

/// 读活跃快照 gen（内部）：[`ACTIVE`] Acquire 载入映射到对应 [`Snap`] 取
/// `.gen`；哨兵态自然落到 [`EMPTY_SNAP`]（gen = 0）。写点在 IRQ 屏蔽窗内
/// （同 [`bind`] 纪律）——单核读窗内无换表，无竞态。
fn self_gen_now() -> u64 {
    // SAFETY: 指针源自 active_snap（静态区槽位/哨兵，'static 生存期）；
    // 已发布槽内容不可变（写者只写非活跃且无 hazard 槽，②）+ 读窗串行
    // 互斥——本读窗内快照稳定（读纪律同 apply/weight_of，仅多读一个 gen
    // 字段）。
    unsafe { (*active_snap()).gen }
}

/// 当前活跃快照的世代数（读点观察用）：未绑（哨兵 [`EMPTY_SNAP`]）→ 0，
/// 已绑 → 最后一次 [`bind`]/[`bind_gen`] 发布的 gen。v3 `policy_bind`
/// 换表协议世代词汇的读侧——写者带 gen 重放对账、遥测基准值。
pub fn gen() -> u64 {
    self_gen_now()
}

/// 带世代的换表 + 陈旧拒收（v3 `policy_bind` 换表协议的写侧底座）：传入
/// gen 必须**严格大于**当前活跃世代（`gen <= self_gen_now()` 即拒收）——
/// 写者重放/乱序到达的旧表不得覆盖新表，世代单调性在此收口。受理则用
/// **传入 gen**（非 [`GEN_NEXT`] 自增——那是 [`bind`] 的自动通道）
/// build_snap + publish，gen 记入 [`Snap::gen`]。输入放宽到非 `'static`
/// `&[Entry]`：快照化后条目 build 时全量拷入 [`Snap`]，发布即与调用方
/// 数据解耦，无需静态生存期。
///
/// 返回：true = 换表成功；false = 陈旧拒收（活跃快照未动）。写者入口
/// （③）：gen 检查与 publish 同在 [`POLICY_LOCK`] 窗内——两核写者交错
/// 不会把「检查过的新 gen」写进「已被更新的表」（检查+写入原子性在此收口）。
pub fn bind_gen(tbl: &[Entry], gen: u64) -> bool {
    policy_lock(); // gen 检查 + 写入的复合临界区（③）：锁内原子
    if gen <= self_gen_now() {
        policy_unlock(); // 错误路径同解锁（early return 不泄漏锁）
        return false; // 陈旧拒收：世代必须严格递增
    }
    publish(build_snap(tbl, gen));
    policy_unlock();
    true
}

/// 解绑：翻转 [`ACTIVE`] 回 [`SENTINEL_EMPTY`] 哨兵（读路径回落
/// [`EMPTY_SNAP`]，恢复「接口缺席」态——等价性测试的对照分支；ktest 收尾
/// 清理）。
pub fn unbind() {
    ACTIVE.store(SENTINEL_EMPTY, Ordering::Release);
}

/// 接口是否在位（测试观察用）：已发布快照（含**空表**——接口在位、表空
/// 是等价性测试的主断言态）即 true，哨兵态 false。
pub fn bound() -> bool {
    ACTIVE.load(Ordering::Acquire) != SENTINEL_EMPTY
}

/// 决策路径挂钩：pick_next 出选后调用。查到指令 → 写 TCB 表示法字段
/// （**幂等**：`prio_effective = max(prio_base + 夹紧权重, pi_inherited)`
/// 自基线重算、EPP 直接赋值——非累积，重复 pick 不漂移）；查不到 → **零
/// 写入**，字段保持基线，与接口缺席逐位等价（ktest `POLICY ok` 断言对象）。
/// IRQ 热路径零分配。快路径：哨兵/空表两次分支即返（§2「开销恒定」的
/// 空态兑现）。v1.3 起顺带消费 ①放置偏置：SMP 参与门开且 placement≠0
/// 才写 `Task::placement_hint`（单核 = 零写入——空表等价纪律延伸到放置
/// 面；**只存不路由**，唤醒选核的真消费面随 SMP 路由接，见函数尾注）。
///
/// **shadow 对账挂最前**（每次调用都对账，active 决策逻辑零改动）：
/// [`shadow_reconcile`] 只读只计数——shadow 谱系空时一次 gen 读即返，热
/// 路径开销恒定。须在 active 早退路径**之前**：active 空表/未命中的 pick
/// 也要对账（demote 后 active 空、shadow 有表正是双轨观测的主场景）。
pub fn apply(task: &mut Task) {
    shadow_reconcile(task);
    // SAFETY: 指针源自 active_snap（静态区槽位/哨兵，'static 生存期）；
    // 已发布槽内容不可变（写者只写非活跃且无 hazard 槽——本核在读槽号
    // 已经 active_snap 发布，②）+ 读窗串行互斥（现状屏蔽窗/KL）——读窗
    // 内快照稳定，本调用内无写者踩踏。
    let snap: &Snap = unsafe { &*active_snap() };
    if snap.count == 0 {
        return; // 空表/哨兵：零写入快路径
    }
    let key = task.proc_tag as u64;
    if key == 0 {
        return; // 哨兵键：内核任务不可注入（同旧表语义边界收紧，见模块注）
    }
    let slot = match lookup_o1(snap, key) {
        Some(s) => s,
        None => return, // 未命中：零写入
    };
    let w = slot.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX);
    // 4-3b：策略权重与 PI 捐赠同合成（冻结的 boost 不被 apply 抹平）。
    task.prio_effective = (task.prio_base + w).max(task.pi_inherited);
    // EPP 解码单源 = `berry_polface::epp_unpack`（两位打包律 W3-Q20 外迁）。
    if let Some(e) = epp_unpack(slot.epp_flags) {
        task.epp = e;
    }
    // v1.3 EAS-1 首刀（`调点调度-v1.3` §6 placement 第一消费者·基础设施）：
    // ①放置偏置的 TCB 落点——**只存不路由**（唤醒选核的真路由面等
    // smoke-smp 三相 LB 稳定后再接）。单核形态（SMP 参与门关）→ 零写入：
    // placement 有值 = 无操作，不改变任何行为（空表等价纪律延伸到放置面）。
    // 短路序 = placement==0 先判：无放置偏置的常见态零新增原子读。
    if slot.placement != 0 && hal::smp::par_enabled() {
        task.placement_hint = slot.placement;
    }
}

/// 该任务当前绑定的夹紧权重（未绑/无表/空表 → 0）。[`crate::sched::
/// refresh_effective`] 的合成分量——PI 重推刷 effective 时与 pick 侧 apply
/// 用同一份权重，两写入者公式一致才不会互相踩（4-3b）。
pub(crate) fn weight_of(task: &Task) -> i32 {
    // SAFETY: 读纪律同 apply（静态区指针 + 已发布槽不可变）。
    let snap: &Snap = unsafe { &*active_snap() };
    if snap.count == 0 {
        return 0;
    }
    let key = task.proc_tag as u64;
    if key == 0 {
        return 0;
    }
    match lookup_o1(snap, key) {
        Some(s) => s.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX),
        None => 0,
    }
}

/// 该 key 当前绑定的放置偏置（动作空间①：`调点调度-v1.3` §6 placement
/// 第一消费者——唤醒路径核选择的读出口）：读活跃快照 O(1) 查表透传
/// [`SnapSlot::placement`]。三值词汇表：**0 = 无意见**（默认——未绑/空表/
/// 未命中/哨兵键，与接口缺席等价）、1 = 偏 little（省电核 CPU0）、2 = 偏
/// big（性能核 CPU1）。越域注入值原样透传（v1 注入面 = 受信内核侧，词汇
/// 执法随 SMP 真消费面定）。v1.3 首刀只立读出口——「placement 值经策略表
/// 正确可达」的 ktest 断言对象；唤醒选核的真消费随 SMP 路由（smoke-smp
/// 三相 LB 稳定后）接，单核形态下 placement 有值 = 无操作。
///
/// **overutilized 门控**（§7 回退——EAS 永远是机会主义）：任一核 util 超
/// 域最高容量 80%（[`crate::util::any_overutilized`]）→ 恒 0（放置 hint
/// 失效）——饱和系统里能量/放置比较失真（所有候选都「贵」），回退普通
/// 负载均衡（= 无 hint 默认行为）。Linux EAS 同名纪律：overutilized 时
/// 能量感知选核整域让位。
pub fn placement_hint(task_key: u64) -> u8 {
    // overutilized 回退（v1.3 §7）：任一核超容量 80% → 放置 hint 失效
    //（短路序 = 门控最前：饱和态下表查不查都无意义，热路径省一次快照读）。
    if crate::util::any_overutilized() {
        return 0; // 无意见（= 回退到无 hint 的默认行为）
    }
    // SAFETY: 读纪律同 apply/weight_of（active_snap 静态区指针 + 已发布槽
    // 不可变 + 单核屏蔽窗读写互斥——读窗内快照稳定）。
    let snap: &Snap = unsafe { &*active_snap() };
    if snap.count == 0 {
        return 0; // 未绑（哨兵）/空表：无意见
    }
    match lookup_o1(snap, task_key) {
        Some(s) => s.placement,
        None => 0, // 未命中/哨兵键（0 保留，lookup 早退）：无意见
    }
}

// ---- Shadow 谱系：双轨道决策计算（COMMIT + PROMOTE/DEMOTE）----
//
// 双轨道隔离的**结构性**保证（约束①：快路径零改动）：[`ACTIVE`] 的值域只有
// 0..=2/哨兵——经 [`active_snap`] 永远只达 [`SNAPS`]/[`EMPTY_SNAP`]，shadow
// 槽对快路径**不可达**（非口头约定，是地址空间上无别名）。shadow 侧写纪律
// 同 [`SNAPS`]（约束②）：只写非活跃槽 + 翻转 [`SHD_WHERE`] 发布——翻转位
// 自 SMP 安全三件套①（原子化）起为 AtomicU8（写侧「槽写 +
// fence(Release) + store(Release)」/ 读侧 Acquire），槽数据仍裸写；
// 写者互斥自三件套③起由 [`POLICY_LOCK`] 串行化（bind_gen_shadow/
// promote 写/读 shadow 槽全程持锁）；hazard-lite 槽回收（②）active 三槽
// 侧已落 HAZARD、shadow 双槽侧未覆盖——shadow 读（shadow_gen/对账）与
// shadow 换表的跨核并发仍靠现状串行，留后续刀。双槽
// （而非三槽）即足——shadow 无快路径读者，翻转后旧槽只剩写者自己会再碰
// （约束③的拷贝源/目的地都在写者手里）。
//
// **divergence 对账**（计数器/判定见文件尾「Shadow divergence 对账」节）：
// [`apply`] 每调一次，shadow 谱系非空即对同一 key 双表各算一份决策、只读
// 对比**只计数**（shadow 差异永不应用——只算不用原则的观测面延伸）；轨道
// 切换（promote/demote）自动清零，对账期与轨道绑定。

/// shadow 双槽轮转区：写者写 `SHD[SHD_WHERE ^ 1]`（非活跃槽）→ 翻转
/// [`SHD_WHERE`] 发布。`static mut` 纪律同 [`SNAPS`]（`&raw mut` 取裸指针，
/// 不立引用——E0596）。
static mut SHD: [Snap; 2] = [const { Snap::empty() }; 2];
/// 当前活 shadow 槽号（0/1）。写者翻转本位 = shadow 侧发布。SMP 安全
/// 三件套①（原子化）：AtomicU8——写侧「先写槽 + fence(Release) +
/// store(Release)」、读侧 load(Acquire)，见新槽号必见新槽内容；单核
/// Acquire/Release 无重排面 = 行为零变化。③写者锁已落（写者入口
/// [`POLICY_LOCK`] 串行化——见文件头锁注）；②hazard-lite 槽回收 shadow
/// 侧未覆盖（active 侧见 HAZARD）——shadow 槽数据的跨核读写仍靠现状
/// 串行，留后续刀。
static SHD_WHERE: AtomicU8 = AtomicU8::new(0);

/// shadow 活槽裸指针（读窗纪律同 [`active_snap`]：静态区槽位、'static 生存期
/// ——解引用读窗的 SAFETY 在调用点挂）。
fn shadow_snap() -> *const Snap {
    // Acquire 读 = 与 bind_gen_shadow 的「槽写 + fence(Release) +
    // store(Release)」发布序配对：见新槽号即见新槽内容（三件套①）。
    // `& 1` 收口防越界（冗余防线，槽号 < 2 编译期可证）。
    let slot = SHD_WHERE.load(Ordering::Acquire) & 1;
    // SAFETY: `static mut` 取 `&raw const` 裸指针（不立引用——E0133 解除的
    // 就是这一层）。
    unsafe { &raw const SHD[slot as usize] }
}

/// shadow 侧 gen 观察（双轨道遥测/对账）：从未绑定 → 0（[`Snap::empty`]
/// 基底），已绑 → 最后一次 [`bind_gen_shadow`] 受理的 gen。读窗纪律同
/// [`self_gen_now`]（单核屏蔽窗读写互斥——读窗内稳定）。
pub fn shadow_gen() -> u64 {
    // SAFETY: 指针源自 shadow_snap（静态区槽位，'static 生存期）；已发布
    // shadow 槽内容不可变（写者只写非活跃槽）+ 单核屏蔽窗读写互斥——本读
    // 窗内快照稳定（读纪律同 self_gen_now，仅多读一个 gen 字段）。
    unsafe { (*shadow_snap()).gen }
}

/// 绑 shadow 表（COMMIT + SHADOW flag 语义——双轨道写侧）：校验 +
/// [`build_snap`]（gen 须**严格大于** shadow 侧当前 gen——陈旧拒收同
/// [`bind_gen`] 语义，但比照对象换 **shadow 谱系**，与 active 侧 gen 无关）
/// → 写非活跃 shadow 槽 → 翻转 [`SHD_WHERE`]。**ACTIVE 不动**：shadow 只算
/// 不用，决策路径（apply/weight_of）零感知（约束①）。
///
/// 返回：true = shadow 表已换；false = 陈旧拒收（shadow 谱系未动）。写者
/// 入口（③）：shadow gen 检查 + 写槽 + 翻转 [`SHD_WHERE`] 同在
/// [`POLICY_LOCK`] 窗内——检查与发布原子，与他核 promote 读 shadow 槽
/// 互斥（promote 同持本锁，见锁序注）。
pub fn bind_gen_shadow(tbl: &[Entry], gen: u64) -> bool {
    policy_lock(); // shadow gen 检查 + 写槽 + 翻转的复合临界区（③）
    if gen <= shadow_gen() {
        policy_unlock(); // 错误路径同解锁（early return 不泄漏锁）
        return false; // 陈旧拒收：shadow 世代必须严格递增（同 bind_gen 语义）
    }
    let snap = build_snap(tbl, gen);
    // 写者读自身上次发布值选非活跃槽（锁内写者自见，Relaxed 足够）。
    let next = SHD_WHERE.load(Ordering::Relaxed) ^ 1;
    // SAFETY: POLICY_LOCK 独占写者（③）写**非活跃** shadow 槽——shadow
    // 无快路径读者（ACTIVE 不指向 SHD）、锁内写者自身此窗不读该槽，写窗内
    // 无别名引用；static 存储期 = 整个程序运行期，ptr::write 不越界（槽号
    // < 2 编译期可证）。
    unsafe {
        core::ptr::write(&raw mut SHD[next as usize], snap);
    }
    // 发布序（三件套①）：槽写在前、fence(Release) 定序、Release store 翻转
    // 在后——读者（shadow_snap 的 Acquire load）见新槽号必见新槽内容。
    // 单核 Release = 普通 u8 写，零行为变化。
    fence(Ordering::Release);
    SHD_WHERE.store(next, Ordering::Release);
    policy_unlock();
    true
}

/// shadow → enforced 原子转正（PROMOTE）：读当前 shadow 活槽 → **一次 ~4KiB
/// 受控拷贝**（`Snap` 按值 4112B，源/目的地均在写者手里的静态区——无逃逸、
/// 无分配、非共享数据竞争面）→ [`publish`] 单条 Release store 原子可见
/// （约束③：机制面完全复用三槽轮转 publish，不新开同步原语）。gen 取
/// shadow 侧的 gen：转正后 active 谱系 gen = shadow gen——此后 active 侧
/// [`bind_gen`] 的陈旧拒收比照该值。
///
/// 返回：true = 已转正（ACTIVE 指向新表，立即参与决策）；false = shadow
/// 表为空（gen == 0，从未 [`bind_gen_shadow`] 受理）——无表可转正，ACTIVE
/// 不动。转正即**轨道切换**：对账计数自动清零（[`divergence_reset`]——
/// shadow 表已成 active，此后偏差开新对账期从零计；false 支不切换不清零）。
/// 写者入口（③）：shadow 读 + 4KiB 拷贝 + publish 同在 [`POLICY_LOCK`]
/// 窗内——与 [`bind_gen_shadow`] 的 shadow 换表互斥，拷贝源不被中途覆写。
pub fn promote() -> bool {
    policy_lock(); // shadow 读 + 拷贝 + publish 的复合临界区（③）
    let src = shadow_snap();
    // SAFETY: 指针源自 shadow_snap（静态区槽位，'static 生存期）；③写者锁
    // 窗内独占——读窗内 shadow 槽稳定。ptr::read 按值拷出（4112B 受控
    // 拷贝），源槽内容保留在 shadow 谱系（demote 后可再次 promote 复转正）。
    unsafe {
        if (*src).gen == 0 {
            policy_unlock(); // 错误路径同解锁（false 支不切轨道不清零）
            return false; // shadow 空（从未绑定）：无表可转正
        }
        publish(core::ptr::read(src));
    }
    divergence_reset(); // 轨道切换 = 新对账期（仅成功支）
    policy_unlock();
    true
}

/// enforced → shadow 降级（DEMOTE）：单条 Release store 把 [`ACTIVE`] 打回
/// [`SENTINEL_EMPTY`]——读路径立即回落 [`EMPTY_SNAP`]，等价 [`unbind`] 的
/// 「接口缺席」态（apply/weight_of 空表两次分支即返、零写入——约束④：active
/// 立即回空表语义）。**shadow 表保留**（SHD/SHD_WHERE 不动）：双轨道计算
/// 可继续，随时 [`promote`] 复转正。降级即**轨道切换**：对账计数自动清零
/// （[`divergence_reset`]）——此后 active 空 vs shadow 有表，每次命中都是
/// divergence，降级态偏差从零观测。写者入口（③）：ACTIVE 打回 + 对账清零
/// 同在 [`POLICY_LOCK`] 窗内——轨道切换不与他核写者交错。
pub fn demote() {
    policy_lock();
    ACTIVE.store(SENTINEL_EMPTY, Ordering::Release);
    divergence_reset(); // 轨道切换 = 新对账期
    policy_unlock();
}

// ---- Shadow divergence 对账：shadow「会做什么」vs active「做了什么」----
//
// 语义：[`apply`] 每次调用时，shadow 谱系非空（gen > 0）即对**同一 task
// key** 双表各算一份消费点决策（夹紧权重 + EPP 覆盖位；未命中/空表/key 0 =
// 「无意见」），决策不同（weight **或** EPP 差异）只**计数**不应用——shadow
// 侧零 TCB 写、零表写（纯读 + 原子计数），active 决策与写入逻辑零改动。
// 计数是**对账期**口径：轨道切换（promote/demote）自动清零，新旧表的偏差
// 不混账；divergence_count / shadow_eval_count = 当前对账期的 divergence 率。

/// 对账期内 shadow/active 决策不一致次数（weight 或 EPP 差异）。
/// **per-CPU 分账（SMPD-4）**：写方限本核（cpu_id 索引）零跨核争用；读时
/// 合并（各核只增不减直和无损）。64B 对齐防伪共享（HDR CpuHdr 同款纪律）。
const RECON_CPUS: usize = 2;

#[repr(C, align(64))]
struct CountSlot(AtomicU64);
const _: () = assert!(core::mem::align_of::<CountSlot>() == 64);
const _: () = assert!(core::mem::size_of::<CountSlot>() == 64);
static DIVERGENCE_PER_CPU: [CountSlot; RECON_CPUS] =
    [const { CountSlot(AtomicU64::new(0)) }; RECON_CPUS];
/// 对账期内 shadow 决策评估次数（shadow 谱系非空时的 apply 调用数）——
/// divergence 率的分母。
static EVAL_PER_CPU: [CountSlot; RECON_CPUS] =
    [const { CountSlot(AtomicU64::new(0)) }; RECON_CPUS];

// —— 拒收入账（P1-a，吸收调研 24 号（原 12）astrid「审计链含 deny」精神）——
// 注入受理面的四类静默处置从此有账：拒收/截尾/窗满/夹取各一计。写点在
// bind/unbind（boot/ktest 期 SVC 屏蔽、单写者），Relaxed 纯统计同
// divergence 对账形态——决策路径（apply/weight_of）零新增写面。
/// key==0 哨兵键丢弃数（proc_tag=0 的内核任务不可注入——语义性丢弃）。
static REJECT_KEY0: AtomicU64 = AtomicU64::new(0);
/// build 期 TBL_MAX 截尾丢弃的条目数（表超载——注入方一次给太多）。
static REJECT_OVERFLOW: AtomicU64 = AtomicU64::new(0);
/// 探测窗满拒收数（散列聚簇到 PROBE_MAX 仍无空槽——O(1) 上界的代价面）。
static REJECT_PROBE_FULL: AtomicU64 = AtomicU64::new(0);
/// weight 被包络夹取数（注入越 ±WEIGHT_MAX 被夹——护栏生效频率的量化）。
static CLAMP_WEIGHT: AtomicU64 = AtomicU64::new(0);

/// 拒收/夹取面快照（遥测/ktest 消费；v3 接环前的读出口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectStats {
    pub key0: u64,
    pub overflow: u64,
    pub probe_full: u64,
    pub clamped: u64,
}

/// 读拒收入账四计数（Relaxed 快照——纯统计，无重置面；对账期语义归
/// divergence 族，本账自 bind 以来单调累计）。
pub fn reject_stats() -> RejectStats {
    RejectStats {
        key0: REJECT_KEY0.load(Ordering::Relaxed),
        overflow: REJECT_OVERFLOW.load(Ordering::Relaxed),
        probe_full: REJECT_PROBE_FULL.load(Ordering::Relaxed),
        clamped: CLAMP_WEIGHT.load(Ordering::Relaxed),
    }
}

/// 单次对账（apply 每调一次、shadow 谱系非空时生效）：同 key 双表决策对比
/// + 计数。**只读不写**——不写 TCB（差异不应用）、不写表（shadow 侧纯
/// 观察）。谱系空时一次 gen 读即返：apply 热路径开销恒定（空 shadow 的
/// 快路径零感知——约束①在对账侧的兑现）。
fn shadow_reconcile(task: &Task) {
    // SAFETY: 指针源自 shadow_snap（静态区槽位，'static 生存期）；已发布
    // shadow 槽内容不可变（写者只写非活跃槽）+ 单核屏蔽窗读写互斥——本读
    // 窗内快照稳定（读纪律同 shadow_gen）。
    let shd: &Snap = unsafe { &*shadow_snap() };
    if shd.gen == 0 {
        return; // shadow 谱系空：一次 gen 读 + 分支即返（不进对账）
    }
    // SAFETY: 读纪律同 apply（active_snap 静态区指针 + 已发布槽不可变）。
    let act: &Snap = unsafe { &*active_snap() };
    // key 0（内核任务）：lookup_o1 早退返 None → 双方同为「无意见」，计入
    // eval 不可能 divergence——与注入侧「0 键丢弃」同一语义边界。
    let key = task.proc_tag as u64;
    let a = lookup_o1(act, key);
    let s = lookup_o1(shd, key);
    // 消费点语义的决策：命中取夹紧权重（同 apply/weight_of 的第三道夹紧），
    // 未命中/空表 = 0。
    let aw = a.map_or(0, |sl| sl.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX));
    let sw = s.map_or(0, |sl| sl.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX));
    // EPP 决策 = epp_flags 整字节（bit0 有无覆盖 + bit1 取值两位打包）——
    // None↔Some、Efficiency↔Perf 的差异都落在字节差异上。
    let ae = a.map_or(0, |sl| sl.epp_flags);
    let se = s.map_or(0, |sl| sl.epp_flags);
    let cpu = hal::smp::cpu_id().min(RECON_CPUS - 1) as usize;
    if aw != sw || ae != se {
        DIVERGENCE_PER_CPU[cpu].0.fetch_add(1, Ordering::Relaxed);
    }
    EVAL_PER_CPU[cpu].0.fetch_add(1, Ordering::Relaxed);
}

/// 对账期内决策不一致次数（shadow 会做的 ≠ active 做了的：weight 或 EPP
/// 差异）。遥测口径：`divergence_count() / shadow_eval_count()` = 当前
/// 对账期的 divergence 率。
pub fn divergence_count() -> u64 {
    DIVERGENCE_PER_CPU[0].0.load(Ordering::Relaxed)
        + DIVERGENCE_PER_CPU[1].0.load(Ordering::Relaxed)
}

/// 对账期内 shadow 决策评估次数（divergence 率的分母）。
pub fn shadow_eval_count() -> u64 {
    EVAL_PER_CPU[0].0.load(Ordering::Relaxed)
        + EVAL_PER_CPU[1].0.load(Ordering::Relaxed)
}

/// 清零对账计数（两个计数器同清——新对账期从零起算）。[`promote`]/
/// [`demote`] 轨道切换时自动调用；测试观察亦可用。
pub fn divergence_reset() {
    for i in 0..RECON_CPUS {
        DIVERGENCE_PER_CPU[i].0.store(0, Ordering::Relaxed);
        EVAL_PER_CPU[i].0.store(0, Ordering::Relaxed);
    }
}
