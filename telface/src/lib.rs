//! berry-telface —— telemetry 行编码与聚合纯函数面（W3-Q10）。
//!
//! **动机**：遥测行的数值编码律此前散在 `kernel/src/telemetry.rs`（name_id
//! FNV-1a 哈希 / log2 延迟桶律 `63 - leading_zeros`）与 `kernel/src/user.rs`
//! （`benchwake_percentile` 分位秩——直方图桶 → 保守分位上界）里——host 测
//! 不可达（kernel 依赖 HAL global_asm，只 aarch64-unknown-none 可编），ktest
//! 只覆盖 boot 相。编码律的回归面（哈希稳定性、桶边界、分位秩单调、行格式
//! 不变量）需要一张 host 可测的**钉板**：本 crate 把这些纯函数按值域抽成
//! 单源 + 表驱动测试，`kernel` 侧消费同一函数（零第二实现——抽离即唯一
//! 事实源，非镜像；hdlface encode/decode 同款纪律）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`name_id`]：任务名 → 稳定短 id（FNV-1a-32）——环条目 `prev`/`next`/
//!   `task_id` 与槽 `last_woken_id` 的编码器；**稳定性 = 跨 boot/跨编译器
//!   不变**（展示端反查名字表靠它，改算法 = 撕裂既有日志对照面）。
//! - [`HIST_BUCKETS`] 与 [`latency_bucket`]：R-0 唤醒延迟直方图的 log2 桶
//!   律（延迟 ∈ [2^i, 2^(i+1)) 归桶 i；0 归桶 0；≥ 2^63 归桶 63——无下溢
//!   无溢出）。
//! - [`bucket_upper_bound`]：桶 i 的保守分位上界（`1 << (i+1)`，顶桶
//!   `u64::MAX`——2^64 溢出防）。
//! - [`hist_percentile`]：直方图分位秩（最近秩 ceil(p·n/100) 的 log2 形
//!   ——user.rs `benchwake_percentile` 单源迁此；p50/p90/p99 消费）。
//! - [`WakesrcRow`] / [`wakesrc_row`]：G1.5 WAKESRC 单行的**字段序律**钉板
//!   （acct/T/U/R/L/I/K/P/X 九段 + U/P 三段形 `w/i/nw`、其余 `w/i` 两段
//!   形——行格式的字段序与段形是 smoke 收割正则的契约面，散拼一处错序
//!   即红一整轮 CI）。
//! - [`MemstatRec`] / [`TELEMETRY_OP_KLOG`] / [`TELEMETRY_OP_MEMSTAT`]：
//!   0x84 telemetry_open op 域的出参形状与 op 常量单源（遥测 v3 首件
//!   memstat 定长快照 40B——内核派发与 berry-sys 包装同引此，消内核/
//!   用户双写；add-only 扩路见 [`MemstatRec`] 注）。
//! - [`SwitchRec`]/[`WakeRec`]/[`RingsHdrV1`]/[`RingSegV1`] 与
//!   [`TELEMETRY_OP_RINGS`]：op=2 双族环快照的形状与常量单源（环读口刀
//!   ——自 kernel/src/telemetry.rs 迁此；头/段 LE 编解码纯函数与
//!   `ringmerge::fill_span` 旋转律同批，host 钉板）。
//! - [`WakeSrcRecV1`]/[`WsrcHdrV1`] 与 [`TELEMETRY_OP_WSRC`]：op=3
//!   WakeSrc 槽表纯镜像的形状与常量单源（槽表读口刀——40B 轻头＋
//!   128×64B 定长行，行号即源值零 src 域；kernel 原子壳同偏移镜像由
//!   cross-const assert 锁步；头/行 LE 编解码纯函数，host 钉板；
//!   berryctl powertop 消费）。
//!
//! 与冻结面的关系：NR 表/派发面零触碰（0x84 op 域常量与出参形状是唯一
//! ABI 面，即上条单源项）；环/槽表的原子机械（seqlock 写方
//! 协议、fetch_min/max 序）不在此 crate（有状态并发面留 kernel——本面只
//! 钉「值怎么编码」的判定律，不钉「环怎么写」的机械）。
//!
//! 与 berry-chanface 的域界（W3-Q9 定谳）：chanface 钉 channel 队列侧
//! 判据；本 crate 钉 telemetry 编码侧——两 crate 域不重叠。

#![cfg_attr(not(test), no_std)]

// ---- name_id：FNV-1a-32 稳定哈希（单源：自 kernel/src/telemetry.rs 迁此，
// kernel 侧 `use berry_telface::name_id` 消费——原私有 fn 删除，零第二
// 实现）----

/// 任务名 → 稳定短 id（FNV-1a-32；0 = idle/boot 前，展示端反查靠名字表，
/// 环内只存定宽 id）。
///
/// 稳定契约：同名字恒同 id（offset basis 0x811c_9dc5 + prime 0x0100_0193，
/// 逐字节 xor-then-mul；空名 = basis 0x811c_9dc5 非 0——idle 判据靠名字表
/// 而非 id 值，此函数不承担语义判）。算法参数是**日志对照面的契约常量**
/// ——改动 = 既有串口日志/环内 id 全部失效，禁止。
#[inline]
pub fn name_id(name: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in name {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

// ---- R-0 直方图 log2 桶律（单源：自 kernel/src/telemetry.rs 迁此；
// WAKE_HIST_BUCKETS 常量 kernel 侧保留（telemetry.rs 静态数组长度承重），
// 桶函数与桶数语义在此钉板）----

/// log2 延迟桶数（桶 i 计延迟 ∈ [2^i, 2^(i+1)) 的唤醒次数；延迟 0（同刻
/// 醒）归桶 0，延迟 ≥ 2^63 归桶 63——打印端上界给 u64::MAX 防 2^64 溢出）。
pub const HIST_BUCKETS: usize = 64;

/// 延迟 → log2 桶下标（on_wake 写点律：lat=0 归桶 0；否则 idx = 63 −
/// leading_zeros——lat ≥ 1 ⇒ lz ≤ 63，lat ≥ 2^63 恰归桶 63，无下溢无溢出）。
#[inline]
pub fn latency_bucket(lat: u64) -> usize {
    if lat == 0 {
        0
    } else {
        63 - lat.leading_zeros() as usize
    }
}

/// 桶 i 的保守分位上界（桶 i 覆盖 [2^i, 2^(i+1))——上界 = 下一桶底；
/// 顶桶 i=63 的上界 = u64::MAX 而非 2^64（溢出防）。[`hist_percentile`] 的
/// 返回值即此——「分位是桶上界的保守近似」的诚实取舍单源。
#[inline]
pub fn bucket_upper_bound(idx: usize) -> u64 {
    if idx >= 63 {
        u64::MAX
    } else {
        1 << (idx + 1)
    }
}

/// 直方图分位（最近秩 ceil(p·n/100) 的 log2 形）：返回覆盖该秩的最小桶的
/// 保守上界。
///
/// 单源自 `kernel/src/user.rs::benchwake_percentile`（p50 = (1,2)、p90 =
/// (9,10)、p99 = (99,100) 消费）——`num/den` 有理数形避免浮点（no_std +
/// 确定性）。`sum(buckets) ≥ n ≥ target` 恒立时循环必命中；空直方图
/// （n=0 → target=0 → 首桶即命中）与溢出面（saturating_mul + 保守回
/// u64::MAX）的防御路径原样保留。
#[inline]
pub fn hist_percentile(buckets: &[u64], n: u64, num: u64, den: u64) -> u64 {
    let prod = n.saturating_mul(num);
    let target = prod / den + u64::from(prod % den != 0);
    let mut acc: u64 = 0;
    for (idx, &c) in buckets.iter().enumerate() {
        acc += c;
        if acc >= target {
            return bucket_upper_bound(idx);
        }
    }
    // 不可达（sum ≥ n ≥ target）；保守回 u64::MAX——打印路径非执法点。
    u64::MAX
}

// ---- G1.5 WAKESRC 行字段序律（user.rs watchdog 散拼 → 结构化钉板；
// kernel 侧暂按段序消费（拼行仍 uart 直写——PRINT_LOCK 窗内逐段 puts 是
// G43 契约），本类型钉「段序 + 段形」的判定律，防散拼错序）----

/// WAKESRC 单行九段快照（user.rs G1.5 块的快照面——T/U/R/L/I/K/P/X 八源
/// + acct 总账）。
///
/// 段形律：U（UART_RX）与 P（POWER_KEY）为三段形 `w/i/nw`（no_waiter 计
/// 数面），其余六段两段形 `w/i`——**段形与段序是 smoke 收割正则的契约**，
/// 错序/漏段即整轮 CI 红。字段名与 telemetry `WakeSrcSnapshot` 对齐（此
/// 处独立声明保持本 crate 零依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakesrcRow {
    /// 独立总账（Σ 全槽 wakes——守恒断言右侧）。
    pub acct: u64,
    /// TIMER（wakes, irq）。
    pub t: (u64, u64),
    /// UART_RX（wakes, irq, no_waiter）——三段形。
    pub u: (u64, u64, u64),
    /// SGI_RESCHED（wakes, irq）。
    pub r: (u64, u64),
    /// SGI_TLB（wakes, irq）。
    pub l: (u64, u64),
    /// IPC（wakes, irq=0——无 HAL 臂，恒两段形）。
    pub i: (u64, u64),
    /// KERNEL_BOOT（wakes, irq=0）。
    pub k: (u64, u64),
    /// POWER_KEY（wakes, irq, no_waiter）——三段形（boot 期快照恒 0/0/0，
    /// 注入期数值面由 `PWRKEY unowned n=` 行承载）。
    pub p: (u64, u64, u64),
    /// UNKNOWN（wakes, irq）。
    pub x: (u64, u64),
}

/// WAKESRC 行段序（钉板常量——消费侧拼行按此序，smoke 正则对齐）。
pub const WAKESRC_FIELD_ORDER: [&str; 9] =
    ["acct", "T", "U", "R", "L", "I", "K", "P", "X"];

/// 行形状自检（host 钉板 + kernel 侧拼行前断言面）：九段全在、U/P 三段形、
/// U/P 的 no_waiter 段位不与其余段混淆。
///
/// 空实现护栏：本函数钉「段序常量与段形声明一致」的元律——`U`/`P` 在
/// [`WAKESRC_FIELD_ORDER`] 的下标（2/7）与 `WakesrcRow` 字段序对齐，防止
/// 后续加段时错位。
pub fn wakesrc_row_shape_ok() -> bool {
    WAKESRC_FIELD_ORDER.len() == 9
        && WAKESRC_FIELD_ORDER[0] == "acct"
        && WAKESRC_FIELD_ORDER[2] == "U"
        && WAKESRC_FIELD_ORDER[7] == "P"
        && WAKESRC_FIELD_ORDER[8] == "X"
}

// ---- 0x84 op 域单源（遥测 v3 首件 memstat：形状与 op 常量——内核派发
// （syscall.rs）与 berry-sys 包装同引此，消内核/用户双写）----

/// 0x84 telemetry_open 的 op 域常量（单源）。0 = klog 快照（内核日志
/// 字节环——bsh dmesg 消费面）。
pub const TELEMETRY_OP_KLOG: u64 = 0;

/// 1 = memstat 定长快照（[`MemstatRec`] 40B——遥测 v3 首件）。演进注：
/// SwitchRec/WakeRec 环读口已落地 op=2（[`TELEMETRY_OP_RINGS`]）；后续
/// 波次自 op=3 起按落地序取号，不预留（add-only 纪律下预留号无执法值
/// ——域外恒 -ENOTSUP 与未定义同形）。
pub const TELEMETRY_OP_MEMSTAT: u64 = 1;

/// memstat 定长快照（0x84 op=1 出参——`#[repr(C)]` ABI 形状 day-one 定
/// 型；字段全定宽 u64 对齐 8、LE 直拷，无 usize/bool——40B 整）。
///
/// 字段序 = 阅读序（总量 → 不可动 → 已用 → 可用 → VMO 账）：`free_pages`
/// 是读点派生域（装配方在单一致窗三元组上按 total − reserved − alloc
/// 派生——恒等式按构造闭合，消费方可自验）；`committed_pages` 与
/// `alloc_pages` 分窗读，运行窗两者无恒等式（静止窗恒 committed ≤
/// alloc）。
///
/// add-only 扩路（对外层四十年合同的 op 域版）：加字段 = 尾追新版本域
/// 或取新 op 值，禁改旧结构——旧调用方 len=40 恒收 40B v1 前缀，返回
/// 值自证所得。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct MemstatRec {
    /// 物理页总量（boot 构造窗定容常量，不随负载漂）。
    pub total_pages: u64,
    /// 保留页数（buddy RESERVED 对称水位——内核镜像/dtb 等，永不可分配）。
    pub reserved_pages: u64,
    /// 已分配页数（buddy allocated 水位——RESERVED 恒不计入）。
    pub alloc_pages: u64,
    /// 空闲页数（派生域：total − reserved − alloc）。
    pub free_pages: u64,
    /// VMO 已提交页数（per-page ledger 项数的全局聚合投影）。
    pub committed_pages: u64,
}
const _: () = assert!(core::mem::size_of::<MemstatRec>() == 40);

// ---- 0x84 op 域第三件：双族环快照形状与常量（环读口刀——SwitchRec/
// WakeRec 形状自 kernel/src/telemetry.rs 迁此单源〔挂账 1 断言补齐落点〕；
// 头/段定形＋LE 编解码纯函数。内核装配（telemetry.rs/syscall.rs）与
// berry-sys/berryctl 消费同引此，消内核/用户双写）----

/// 2 = 双族环快照（[`SwitchRec`] 切换环 + [`WakeRec`] 唤醒环——per-CPU
/// 四环一体导出，32B 轻头＋每环 24B 段描述符＋变长载荷）。演进注：后续
/// 波次自 op=3 起按落地序取号，不预留（add-only 纪律下预留号无执法值
/// ——域外恒 -ENOTSUP 与未定义同形）。
pub const TELEMETRY_OP_RINGS: u64 = 2;

/// 段族常量：切换环（`RingSegV1.family` 取值）。
pub const RING_FAMILY_SWITCH: u32 = 0;
/// 段族常量：唤醒环。
pub const RING_FAMILY_WAKE: u32 = 1;

/// 单条切换记录（自 kernel/src/telemetry.rs 迁此单源——数据域偏移零漂
/// vct@0/prev@8/next@12/kind@16/idle_next@17；补显式 `_pad` 尾垫
/// （WakeRec 惯例——防未来加字段撞匿名垫无声漂移），24B 整）。
///
/// add-only 扩路（同 [`MemstatRec`] 注）：加字段 = 尾追新版本域或取新
/// op 值，禁改旧结构——`_pad` 恒 0，旧调用方恒收 v1 前缀，返回值自证
/// 所得。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchRec {
    /// 切换时刻 CNTVCT（原始周期数——换算 ns 需快照头 cntfrq，归消费方）。
    pub vct: u64,
    /// 切出任务 id（名字 FNV-1a-32；0 = idle/boot 前）。
    pub prev: u32,
    /// 切入任务 id。
    pub next: u32,
    /// 切换原因（SwitchKind 的 u8 编码全域 **0..=3**：0=Yield 1=Preempt
    /// 2=Exit 3=Block——四值齐；解码归消费方）。
    pub kind: u8,
    /// 切入是否为 idle 任务（repr(C) bool 1B 0/1 稳定——布局零漂移与
    /// 构造点语义零触优先）。
    pub idle_next: bool,
    /// 定宽对齐垫（ABI 演进保留，恒 0）。
    pub _pad: [u8; 6],
}
const _: () = assert!(core::mem::size_of::<SwitchRec>() == 24);

/// 唤醒原因 u8 编码（`WakeReason` 消费面子集——`Pending` 是入队初值，
/// 永不进环；出现即写路径 bug，读侧拒）。自 kernel/src/telemetry.rs 迁此
/// （telemetry.rs re-export 保 crate 内路径稳定——ktest/user.rs 消费点
/// 零改）。
pub const REASON_SIGNALED: u8 = 1;
/// Timeout 编码（见 [`REASON_SIGNALED`]）。
pub const REASON_TIMEOUT: u8 = 2;

/// 单条唤醒记录（自 kernel/src/telemetry.rs 迁此单源——字面零改：
/// vct@0/since_vct@8/wait_obj@16/task_id@24/reason@28/source@29/_pad@30，
/// 32B 整）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WakeRec {
    /// 唤醒时刻 CNTVCT（减 `since_vct` = block→wake 延迟，原始周期数——
    /// 换算 ns 需快照头 cntfrq，归消费方）。
    pub vct: u64,
    /// 入睡时刻 CNTVCT（`block_current` 戳，与 deadline 同屏蔽窗填）。
    pub since_vct: u64,
    /// 等待对象标识（0 = 纯 sleep；非 0 = 对象地址——展示端反查归消费方）。
    pub wait_obj: u64,
    /// 被唤醒任务 id（名字 FNV-1a-32）。
    pub task_id: u32,
    /// 唤醒原因（[`REASON_SIGNALED`]/[`REASON_TIMEOUT`]）。
    pub reason: u8,
    /// 唤醒归因源（`WAKE_SRC_*`——环内恒 ∈ 6 真源，wake_check ⑤ 执法）。
    pub source: u8,
    /// 定宽对齐垫（ABI 演进保留，恒 0）。
    pub _pad: [u8; 2],
}
const _: () = assert!(core::mem::size_of::<WakeRec>() == 32);

/// 双族环快照头版本（v1 轻头——[`RingsHdrV1`]）。
pub const RINGS_SNAP_VER: u32 = 1;

/// 双族环快照头（0x84 op=2 出参前 32B——v1 轻头：砍 magic〔op 派发＋
/// 单源形状已定身份〕、砍 slept_ns/suspend_count〔电源域字段挂起刀再立
/// ——冻结恒 0 = 伪 ABI 教训〕、砍 generation〔其语义挂烘焙一致像，逐环
/// 窗语义无执法值——随挂账 8 统一裁决另立〕）。
///
/// `hdr_bytes` 是头尾追扩形锚：未来加字段走头尾追，旧消费方按自述跳到
/// 段区；`cntfrq` 是消费方 vct→ns 换算唯一源（公约 5 归消费方但因子
/// 内核给，防硬编码伪 ABI）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct RingsHdrV1 {
    /// 版本（= [`RINGS_SNAP_VER`]）。
    pub version: u32,
    /// 头字节数（= 32——头尾追扩形锚，消费方按此跳到段区）。
    pub hdr_bytes: u32,
    /// CNTFRQ（vct→ns 换算因子——vct 域换算的唯一真源）。
    pub cntfrq: u64,
    /// 环容量（每环槽数——V1 几何 64）。
    pub ring_len: u64,
    /// CPU 数（V1 几何 2；消费方按此自适应段数 = 2×族数）。
    pub ring_cpus: u64,
}
const _: () = assert!(core::mem::size_of::<RingsHdrV1>() == 32);

/// 每环段描述符（24B——**紧随其窗口读落笔** ⇒ counts 恒真：count 是实拷
/// 条数非预探测值，两遍探测法的「头 counts 可撒谎」面结构性不存在）。
///
/// `count` 三态算术可区分：空（`total==0`）／降级（`total>0 ∧ count==0`
/// ∧ 余量足——64 次重试上界后读窗不稳的诚实弃段）／截尾（余量尽，返回
/// 值自证）。丢包可派生 `total − min(total, ring_len)`（轮转淘汰数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct RingSegV1 {
    /// 段族（[`RING_FAMILY_SWITCH`]/[`RING_FAMILY_WAKE`]）。
    pub family: u32,
    /// CPU 序号（0..ring_cpus）。
    pub cpu: u32,
    /// 该环累计写入数（单调不减——写方限本核）。
    pub total: u64,
    /// 段内实含条数（三态见类型注）。
    pub count: u32,
    /// 条目字节宽自述（24/32——消费方按此步进条目区）。
    pub rec_size: u32,
}
const _: () = assert!(core::mem::size_of::<RingSegV1>() == 24);

/// V1 几何：每环槽数（kernel `RING_LEN`/`WAKE_RING_LEN` cross-const
/// assert 同值钉死——拓扑真化/扩容 = 几何 MAJOR 走新 op 值纪律，双改由
/// 断言强制同批）。
pub const TELEMETRY_RINGS_V1_RING_LEN: usize = 64;

/// V1 几何：CPU 数（kernel `NCPUS` cross-const assert 同值钉死）。
pub const TELEMETRY_RINGS_V1_CPUS: usize = 2;

/// op=2 快照满载上界（**字面冻结**：32 头 + 4×24 段 + 2×64×24 切换 +
/// 2×64×32 唤醒——不随 size_of 动；变长填充下实写由返回值自证，本值是
/// 内核 stage 与调用方缓冲的容量下界参考）。
pub const TELEMETRY_RINGS_MAX: usize = 7296;

// ---- 头/段 LE 编解码纯函数（host 钉板 round-trip；kernel 装配与
// berryctl 解析同引此单源。条目（SwitchRec/WakeRec）按域偏移 LE 手解/
// 手装——memstat 臂同款，不在本面）----

/// 头 32B LE 编码（[`RingsHdrV1`] → 字节流）。
pub fn rings_hdr_write(h: &RingsHdrV1, out: &mut [u8; 32]) {
    out[0..4].copy_from_slice(&h.version.to_le_bytes());
    out[4..8].copy_from_slice(&h.hdr_bytes.to_le_bytes());
    out[8..16].copy_from_slice(&h.cntfrq.to_le_bytes());
    out[16..24].copy_from_slice(&h.ring_len.to_le_bytes());
    out[24..32].copy_from_slice(&h.ring_cpus.to_le_bytes());
}

/// 头 32B LE 解码（字节流 → [`RingsHdrV1`]；字段合法性归调用方）。
pub fn rings_hdr_read(buf: &[u8; 32]) -> RingsHdrV1 {
    RingsHdrV1 {
        version: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        hdr_bytes: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        cntfrq: u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]),
        ring_len: u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]),
        ring_cpus: u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]),
    }
}

/// 段描述符 24B LE 编码（[`RingSegV1`] → 字节流）。
pub fn ring_seg_write(s: &RingSegV1, out: &mut [u8; 24]) {
    out[0..4].copy_from_slice(&s.family.to_le_bytes());
    out[4..8].copy_from_slice(&s.cpu.to_le_bytes());
    out[8..16].copy_from_slice(&s.total.to_le_bytes());
    out[16..20].copy_from_slice(&s.count.to_le_bytes());
    out[20..24].copy_from_slice(&s.rec_size.to_le_bytes());
}

/// 段描述符 24B LE 解码（字节流 → [`RingSegV1`]；字段合法性归调用方）。
pub fn ring_seg_read(buf: &[u8; 24]) -> RingSegV1 {
    RingSegV1 {
        family: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        cpu: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        total: u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]),
        count: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
        rec_size: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
    }
}

// ---- 0x84 op 域第四件：WakeSrc 槽表纯镜像形状与常量（槽表读口刀——
// 40B 轻头 WsrcHdrV1＋128×64B 定长平数据镜像 WakeSrcRecV1；行号即源值
// 零 src 域〔值直索引 0..=0x7F 槽域单源 berry-wsrcface〕。内核装配
//（telemetry.rs/syscall.rs）与 berry-sys/berryctl powertop 消费同引此，
// 消内核/用户双写）----

/// 3 = WakeSrc 槽表全表纯镜像（[`WsrcHdrV1`] 40B 轻头＋[`WakeSrcRecV1`]
/// 128 行——行号即源值）。演进注：后续波次自 op=4 起按落地序取号，不预留
///（add-only 纪律下预留号无执法值——域外恒 -ENOTSUP 与未定义同形）。
pub const TELEMETRY_OP_WSRC: u64 = 3;

/// 槽表快照头版本（v1 轻头——[`WsrcHdrV1`]）。
pub const WSRC_SNAP_VER: u32 = 1;

/// 单槽平数据镜像行（0x84 op=3 出参条目——`#[repr(C)]` 64B 定宽；与内核
/// 原子壳 `WakeSrcRec`〔kernel/src/telemetry.rs〕**同偏移镜像**：十字段
/// 逐偏移由 kernel 侧 cross-const assert 钉死（插字段/错序即编译红），
/// 本面 size/align const 断言＋host shape_pinned 再钉一层）。
///
/// 不含 `src` 域：**行号即源值**（值直索引 0..=0x7F——槽数与钳位单源
/// berry-wsrcface，头 slots/rec_size 自述几何）。`first_ts` 哨兵语义随形
///（`u64::MAX` = 未触——内核 fetch_min 写序的镜像）；`ts_domain`/`_pad`
/// 是 v1 冻结保留域（内核侧恒 0 **普通字节非原子**——如实标注：无写点、
/// 镜像照拷即尽，v2 SLEPT 双钟域挂账）。数据域 60B＋repr(C) 对齐垫＝64B
/// 整（显式 `_pad`@55..60＋隐式尾垫 60..64——编解码垫区写零）。
///
/// add-only 扩路（同 [`MemstatRec`] 注）：加字段 = 尾追新版本域或取新
/// op 值，禁改旧结构——旧调用方按头 `hdr_bytes`/`rec_size` 自述走读，
/// 返回值自证所得；**条目形变跨大改走新 op 值纪律**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WakeSrcRecV1 {
    /// 本源唤醒事件数（单调族——on_wake fetch_add）。
    pub wakes: u64,
    /// 本源 IRQ 到达数（单调族——TIMER/UART 到达点 fetch_add；SGI×2/
    /// UNKNOWN = G1.5 boot 尾单点并账 store）。
    pub irq_count: u64,
    /// 到达而无等待者（单调族——UART 良性竞态 / POWER_KEY 结构性无主）。
    pub no_waiter: u64,
    /// 首次唤醒时刻 CNTVCT（哨兵 `u64::MAX` = 未触——单调族 fetch_min）。
    pub first_ts: u64,
    /// 最近唤醒时刻（单调族 fetch_max）。
    pub last_ts: u64,
    /// 最近 IRQ 到达/ack 戳（非单调族——定宽 store，可旧可新不撕）。
    pub last_ack_ts: u64,
    /// 最近被唤醒任务 id（非单调族——名字 FNV-1a-32）。
    pub last_woken_id: u32,
    /// 分类/sticky 位（单调族 fetch_or——`WSRC_F_*` 位面单源 wsrcface）。
    pub flags: u16,
    /// 时戳域（v1 恒 0 = CNTVCT 单域；冻结保留）。
    pub ts_domain: u8,
    /// 定宽垫（ABI 演进保留，恒 0——编码垫区写零）。
    pub _pad: [u8; 5],
}
const _: () = assert!(core::mem::size_of::<WakeSrcRecV1>() == 64);
const _: () = assert!(core::mem::align_of::<WakeSrcRecV1>() == 8);

/// 槽表快照头（0x84 op=3 出参前 40B——v1 轻头；砍 magic〔memstat/rings
/// 双判例：op 派发＋单源形状已定身份〕）。
///
/// `hdr_bytes` 是头尾追扩形锚（同 [`RingsHdrV1`]）；`slots`/`rec_size`
/// 自述几何（消费方自适应行数与条目步进——跨版本走读锚，op=2 `RingSegV1`
/// rec_size 判例）；`cntfrq` 是 vct→ns 换算唯一源（rings 头同款）；
/// `now_vct` 是快照时刻 CNTVCT（差分区间时钟锚＋安静时长分母——装配序
/// 保证 ≥ 任一行内时戳：行读毕后取，CNTVCT 单调律）；`wake_acct_n` 是
/// 独立总账（守恒自验右侧——装配序保证瞬态 Σ行wakes ≥ 本值：N 先读
/// Acquire，写序「槽先 N 末位 Release」的读者侧对偶）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WsrcHdrV1 {
    /// 版本（= [`WSRC_SNAP_VER`]）。
    pub version: u32,
    /// 头字节数（= 40——头尾追扩形锚，消费方按此跳到行区）。
    pub hdr_bytes: u32,
    /// 槽数（= 128——行号即源值，消费方自适应行数）。
    pub slots: u32,
    /// 条目字节宽自述（= 64——消费方按此步进行区）。
    pub rec_size: u32,
    /// CNTFRQ（vct→ns 换算因子——vct 域换算的唯一真源）。
    pub cntfrq: u64,
    /// 快照时刻 CNTVCT（行读毕后取 ⇒ ≥ 任一行内**真实**时戳——未触槽 first_ts 的 u64::MAX 哨兵域排除，审查 note 修）。
    pub now_vct: u64,
    /// 唤醒记账独立总账（N 先读 ⇒ 瞬态 Σ行wakes ≥ 本值）。
    pub wake_acct_n: u64,
}
const _: () = assert!(core::mem::size_of::<WsrcHdrV1>() == 40);

/// V1 几何：槽表槽数（kernel `WAKE_SRC_SLOTS` cross-const assert 同值钉死
///——槽域双改由断言强制同批；值域单源 berry-wsrcface `SLOTS`，本常量钉
/// 快照几何面）。
pub const WSRC_TABLE_V1_SLOTS: usize = 128;

/// V1 几何：单槽行宽（kernel `WakeSrcRec` size cross-const assert 同值
/// 钉死——条目形变跨大改走新 op 值纪律）。
pub const WSRC_TABLE_V1_REC: usize = 64;

/// op=3 快照满载上界（**字面冻结**：40 头 + 128×64 行——不随 size_of 动，
/// [`TELEMETRY_RINGS_MAX`] 判例；全表定长镜像恒整表导出，变长仅整行
/// 截尾、返回值 40+行数×64 自证）。
pub const TELEMETRY_WSRC_MAX: usize = 8232;

// ---- 头/行 LE 编解码纯函数（host 钉板 round-trip；kernel 装配与
// berryctl powertop 解析同引此单源。垫区写零〔规范化编码〕、读方垫区
// 不验——rings_hdr_write/read 判例）----

/// 行 64B LE 编码（[`WakeSrcRecV1`] → 字节流；垫区写零——显式 `_pad`
/// @55..60 与 repr(C) 隐式尾垫 @60..64 恒落零字节，字段域照值）。
pub fn wsrc_row_write(r: &WakeSrcRecV1, out: &mut [u8; 64]) {
    out[0..8].copy_from_slice(&r.wakes.to_le_bytes());
    out[8..16].copy_from_slice(&r.irq_count.to_le_bytes());
    out[16..24].copy_from_slice(&r.no_waiter.to_le_bytes());
    out[24..32].copy_from_slice(&r.first_ts.to_le_bytes());
    out[32..40].copy_from_slice(&r.last_ts.to_le_bytes());
    out[40..48].copy_from_slice(&r.last_ack_ts.to_le_bytes());
    out[48..52].copy_from_slice(&r.last_woken_id.to_le_bytes());
    out[52..54].copy_from_slice(&r.flags.to_le_bytes());
    out[54] = r.ts_domain;
    out[55..64].fill(0);
}

/// 行 64B LE 解码（字节流 → [`WakeSrcRecV1`]；垫区不验——照拷字段域，
/// 字段合法性归调用方）。
pub fn wsrc_row_read(buf: &[u8; 64]) -> WakeSrcRecV1 {
    WakeSrcRecV1 {
        wakes: u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]),
        irq_count: u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]),
        no_waiter: u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]),
        first_ts: u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]),
        last_ts: u64::from_le_bytes([
            buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
        ]),
        last_ack_ts: u64::from_le_bytes([
            buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47],
        ]),
        last_woken_id: u32::from_le_bytes([buf[48], buf[49], buf[50], buf[51]]),
        flags: u16::from_le_bytes([buf[52], buf[53]]),
        ts_domain: buf[54],
        _pad: [buf[55], buf[56], buf[57], buf[58], buf[59]],
    }
}

/// 头 40B LE 编码（[`WsrcHdrV1`] → 字节流；无垫区——数据域恰 40B 整）。
pub fn wsrc_hdr_write(h: &WsrcHdrV1, out: &mut [u8; 40]) {
    out[0..4].copy_from_slice(&h.version.to_le_bytes());
    out[4..8].copy_from_slice(&h.hdr_bytes.to_le_bytes());
    out[8..12].copy_from_slice(&h.slots.to_le_bytes());
    out[12..16].copy_from_slice(&h.rec_size.to_le_bytes());
    out[16..24].copy_from_slice(&h.cntfrq.to_le_bytes());
    out[24..32].copy_from_slice(&h.now_vct.to_le_bytes());
    out[32..40].copy_from_slice(&h.wake_acct_n.to_le_bytes());
}

/// 头 40B LE 解码（字节流 → [`WsrcHdrV1`]；字段合法性归调用方）。
pub fn wsrc_hdr_read(buf: &[u8; 40]) -> WsrcHdrV1 {
    WsrcHdrV1 {
        version: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        hdr_bytes: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        slots: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        rec_size: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        cntfrq: u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]),
        now_vct: u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]),
        wake_acct_n: u64::from_le_bytes([
            buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
        ]),
    }
}

// ---- W3-Q81：环读面（双流 vct 归并/帽律）分模块——见 ringmerge.rs ----
pub mod ringmerge;

#[cfg(test)]
mod tests {
    use super::*;

    // ---- name_id：FNV-1a-32 稳定哈希钉板 ----

    /// 已知向量（独立重算的对照值——算法参数 0x811c_9dc5/0x0100_0193 的
    /// 字面钉板；改参数/改序即红）。
    #[test]
    fn name_id_known_vectors() {
        fn ref_fnv(data: &[u8]) -> u32 {
            let mut h: u32 = 0x811c_9dc5;
            for &b in data {
                h ^= b as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
            h
        }
        // 对照向量：空名 = basis；单字节；多字节任务名（内核任务名形）。
        let cases: &[&[u8]] = &[
            b"",
            b"A",
            b"idle",
            b"watchdog",
            b"console",
            b"task_simcon",
            b"abuser",
            b"rwake",
            b"\xff\x00\x80",
        ];
        for &c in cases {
            assert_eq!(name_id(c), ref_fnv(c), "name_id({:?}) 偏离 FNV-1a-32", c);
        }
        // 字面值钉板（防「对照函数与被测函数同源漂移」——两处独立写死）。
        assert_eq!(name_id(b""), 0x811c_9dc5);
        assert_eq!(name_id(b"a"), 0xe40c_292c);
    }

    /// 稳定性：同输入恒同输出（重复调用 + 切片形变不漂移）。
    #[test]
    fn name_id_stable() {
        let a = name_id(b"ktest:hdr_wake");
        for _ in 0..8 {
            assert_eq!(name_id(b"ktest:hdr_wake"), a);
        }
        // 前缀扩展不改前缀段行为（逐字节流性质——无整块混淆）。
        assert_ne!(name_id(b"ab"), name_id(b"abc"));
        assert_ne!(name_id(b"ab"), name_id(b"ba"));
        // 值域：u32 全域可现（非零段映射后截断仍满域）。
        assert_ne!(name_id(b"x"), name_id(b"y"));
    }

    /// 陷阱向量：xor-then-mul 序（mul-then-xor 伪实现即红——判别性向量）。
    #[test]
    fn name_id_order_discriminating() {
        // 伪实现（mul-then-xor）对照：两实现对 "ab" 必分歧。
        let mut wrong: u32 = 0x811c_9dc5;
        for &b in b"ab" {
            wrong = wrong.wrapping_mul(0x0100_0193);
            wrong ^= b as u32;
        }
        assert_ne!(
            name_id(b"ab"),
            wrong,
            "name_id 与 mul-then-xor 伪实现不可同值（xor-then-mul 序钉板）"
        );
    }

    // ---- log2 桶律 ----

    /// 边界律：lat=0 → 桶 0；2^i（i<63）→ 桶 i；2^(i+1)−1 → 桶 i；
    /// 2^63 → 桶 63；u64::MAX → 桶 63（无下溢无溢出全域钉板）。
    #[test]
    fn latency_bucket_boundaries() {
        assert_eq!(latency_bucket(0), 0, "lat=0（同刻醒）归桶 0");
        for i in 0..63u32 {
            let base = 1u64 << i;
            assert_eq!(latency_bucket(base), i as usize, "2^{i} 归桶 {i}");
            if i >= 1 {
                assert_eq!(latency_bucket(base + 1), i as usize, "2^{i}+1 仍桶 {i}");
            }
            assert_eq!(
                latency_bucket((base << 1) - 1),
                i as usize,
                "2^{}−1（桶内顶）归桶 {i}",
                i + 1
            );
        }
        assert_eq!(latency_bucket(1 << 63), 63, "2^63 恰归桶 63");
        assert_eq!(latency_bucket(u64::MAX), 63, "值域顶归桶 63（无溢出）");
    }

    /// 律式全域抽查：bucket(lat) == floor(log2(lat))（lat ≥ 1 时）。
    #[test]
    fn latency_bucket_matches_log2() {
        let mut lat = 1u64;
        while lat < 1 << 40 {
            assert_eq!(latency_bucket(lat) as u32, 63 - lat.leading_zeros());
            lat = lat.wrapping_mul(3).wrapping_add(7); // 伪随机游走
        }
        // 大值域远端抽查（2^60..2^63）。
        for i in 60..63 {
            assert_eq!(latency_bucket(1u64 << i), i as usize);
            assert_eq!(latency_bucket((1u64 << i) + (1u64 << (i - 1))), i as usize);
        }
    }

    /// 桶上界：bucket_upper_bound(i) = 2^(i+1)（i<63）；顶桶 = u64::MAX。
    #[test]
    fn bucket_upper_bound_law() {
        for i in 0..63 {
            assert_eq!(bucket_upper_bound(i), 1u64 << (i + 1));
        }
        assert_eq!(bucket_upper_bound(63), u64::MAX, "顶桶上界 = u64::MAX（2^64 溢出防）");
        // 越界防御（消费侧误传 63+）：同顶桶（保守收敛，不 panic）。
        assert_eq!(bucket_upper_bound(64), u64::MAX);
    }

    /// 覆盖律：bucket_upper_bound(latency_bucket(v)) ≥ v（全值域保守性
    /// ——分位是桶上界近似的前提）。
    #[test]
    fn bucket_covers_value() {
        let mut v = 0u64;
        for _ in 0..200 {
            assert!(bucket_upper_bound(latency_bucket(v)) >= v);
            v = v.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        }
        for i in 0..64 {
            let base = if i == 0 { 0 } else { 1u64 << (i - 1) };
            let top = if i == 63 { u64::MAX } else { (1u64 << i) - 1 };
            assert!(bucket_upper_bound(i) >= top, "桶 {i} 上界须覆盖桶内顶 {top}");
            let _ = base;
        }
    }

    // ---- hist_percentile：分位秩 ----

    /// 单点直方图：全样本同桶 → 任意分位 = 该桶上界。
    #[test]
    fn percentile_single_bucket() {
        let mut b = [0u64; HIST_BUCKETS];
        b[10] = 32;
        assert_eq!(hist_percentile(&b, 32, 1, 2), 1 << 11); // p50
        assert_eq!(hist_percentile(&b, 32, 9, 10), 1 << 11); // p90
        assert_eq!(hist_percentile(&b, 32, 99, 100), 1 << 11); // p99
    }

    /// 最近秩律：n=32 的 p50 = 第 16 样本（秩 ceil(32·1/2)=16）——恰分
    /// 两个桶时 p50 取**高**桶上界（≥ 半数样本所在桶）。
    #[test]
    fn percentile_rank_rounding() {
        let mut b = [0u64; HIST_BUCKETS];
        b[4] = 15; // 桶 4：15 样本（低）
        b[8] = 17; // 桶 8：17 样本（高）
        // target = ceil(32*1/2) = 16 > 15 → 落桶 8。
        assert_eq!(hist_percentile(&b, 32, 1, 2), 1 << 9);
        // 15 样本恰过 p50 半界（ceil(30/2)=15 ≤ 15）→ 桶 4。
        let mut c = [0u64; HIST_BUCKETS];
        c[4] = 15;
        c[8] = 15;
        assert_eq!(hist_percentile(&c, 30, 1, 2), 1 << 5);
    }

    /// 顶桶样本 → u64::MAX（2^64 溢出防——打印面上界诚实）。
    #[test]
    fn percentile_top_bucket_saturates() {
        let mut b = [0u64; HIST_BUCKETS];
        b[63] = 8;
        assert_eq!(hist_percentile(&b, 8, 99, 100), u64::MAX);
        assert_eq!(hist_percentile(&b, 8, 1, 2), u64::MAX);
    }

    /// n=0（零样本直方图）：target=0 → 首桶命中 = 桶 0 上界 2（消费侧
    /// `h.n > 0` 门控在前，此处钉防御路径确定性）。
    #[test]
    fn percentile_zero_n_deterministic() {
        let b = [0u64; HIST_BUCKETS];
        assert_eq!(hist_percentile(&b, 0, 1, 2), 2);
    }

    /// 单调律：分位参数 p 越大，返回值不减（同一分布）。
    #[test]
    fn percentile_monotone_in_p() {
        let mut b = [0u64; HIST_BUCKETS];
        b[3] = 10;
        b[7] = 10;
        b[20] = 10;
        b[40] = 4;
        let mut prev = 0;
        for k in 1..=100u64 {
            let v = hist_percentile(&b, 34, k, 100);
            assert!(v >= prev, "p={k} 分位 {v} < p={} 的 {prev}（分位单调破）", k - 1);
            prev = v;
        }
    }

    /// 与全排序对照（随机小样本 → 精确秩验证）：target 秩的样本落在桶 X
    /// ⟹ hist_percentile = 桶 X 上界（log2 桶是保守上界近似——恰是
    /// benchwake_percentile 的契约）。
    #[test]
    fn percentile_matches_sorted_rank() {
        // 合成 20 样本：15 个延迟 ~2^5 量级、5 个 ~2^9 量级。
        let samples: [u64; 20] = core::array::from_fn(|i| {
            if i < 15 { 32 + (i as u64) } else { 512 + (i as u64) * 7 }
        });
        let mut b = [0u64; HIST_BUCKETS];
        for &s in &samples {
            b[latency_bucket(s)] += 1;
        }
        // p50（秩 10）与 p90（秩 18）都落在高段（≥15 样本在低桶）：
        // p50 秩 10 ≤ 15 → 低桶 5；p90 秩 18 > 15 → 桶 9。
        assert_eq!(hist_percentile(&b, 20, 1, 2), 1 << 6);
        assert_eq!(hist_percentile(&b, 20, 9, 10), 1 << 10);
        assert_eq!(hist_percentile(&b, 20, 99, 100), 1 << 10);
    }

    /// 溢出面：n·num 饱和（saturating_mul）后仍确定性返回，不 panic。
    #[test]
    fn percentile_saturating_no_panic() {
        let mut b = [0u64; HIST_BUCKETS];
        b[0] = u64::MAX;
        assert_eq!(hist_percentile(&b, u64::MAX, u64::MAX, u64::MAX), 2);
    }

    // ---- WAKESRC 行段序律 ----

    #[test]
    fn wakesrc_field_order_shape() {
        assert!(wakesrc_row_shape_ok(), "段序常量与段形声明错位");
        assert_eq!(WAKESRC_FIELD_ORDER[0], "acct");
        assert_eq!(WAKESRC_FIELD_ORDER[1], "T");
        // U/P 三段形（段形声明在 WakesrcRow——元钉板防加段错位）。
        let row = WakesrcRow {
            acct: 102,
            t: (40, 500),
            u: (10, 30, 2),
            r: (5, 15),
            l: (1, 8),
            i: (44, 0),
            k: (2, 0),
            p: (0, 0, 0),
            x: (0, 3),
        };
        assert_eq!(row.acct, 102);
        assert_eq!(row.u.2, 2, "U 段第三位 = no_waiter");
        assert_eq!(row.p.2, 0, "P 段第三位 = no_waiter（boot 期恒 0）");
        // 守恒律（G1.5 ① 的行内自洽）：acct == Σ wakes。
        let sum = row.t.0 + row.u.0 + row.r.0 + row.l.0 + row.i.0 + row.k.0 + row.p.0 + row.x.0;
        assert_eq!(row.acct, sum, "WAKESRC 行内 acct ≠ Σ 槽 wakes（两路失联）");
    }

    // ---- 0x84 op 域：MemstatRec 形状与 op 常量钉板 ----

    /// 形状钉板：size == 40（5×u64）、align == 8——改字段数/改宽即红
    /// （定义处 const 断言钉编译期，此处 host 面再钉一层执法）。
    #[test]
    fn memstat_rec_shape_pinned() {
        assert_eq!(core::mem::size_of::<MemstatRec>(), 40, "五域 5×u64 = 40B 冻结面");
        assert_eq!(core::mem::align_of::<MemstatRec>(), 8, "字段全定宽对齐 8");
    }

    /// 字段偏移字面钉板（0/8/16/24/32——阅读序 total→reserved→alloc→free→
    /// committed；插字段/错序即红）。
    #[test]
    fn memstat_rec_field_offsets_pinned() {
        assert_eq!(core::mem::offset_of!(MemstatRec, total_pages), 0);
        assert_eq!(core::mem::offset_of!(MemstatRec, reserved_pages), 8);
        assert_eq!(core::mem::offset_of!(MemstatRec, alloc_pages), 16);
        assert_eq!(core::mem::offset_of!(MemstatRec, free_pages), 24);
        assert_eq!(core::mem::offset_of!(MemstatRec, committed_pages), 32);
    }

    /// op 常量值钉板（0/1——add-only 纪律下已发布值永不重排；改值 = 撕
    /// 既有调用方）。
    #[test]
    fn telemetry_op_values_pinned() {
        assert_eq!(TELEMETRY_OP_KLOG, 0);
        assert_eq!(TELEMETRY_OP_MEMSTAT, 1);
        assert_ne!(TELEMETRY_OP_KLOG, TELEMETRY_OP_MEMSTAT);
    }

    // ---- op=2 双族环快照：形状/常量/编解码钉板（环读口刀 F4）----

    /// op 常量四值互异（{0,1,2,3}——add-only 下四档在役值钉死；op=3 槽表
    /// 纯镜像档〔槽表读口刀〕随本刀入役，trio 扩 quad）。
    #[test]
    fn telemetry_op_quad_distinct() {
        assert_eq!(TELEMETRY_OP_WSRC, 3);
        let quad = [
            TELEMETRY_OP_KLOG,
            TELEMETRY_OP_MEMSTAT,
            TELEMETRY_OP_RINGS,
            TELEMETRY_OP_WSRC,
        ];
        for i in 0..quad.len() {
            for j in i + 1..quad.len() {
                assert_ne!(quad[i], quad[j], "op 值撞号（{i} vs {j}）");
            }
        }
    }

    /// SwitchRec 形状钉板：size == 24、字段偏移字面 vct@0/prev@8/next@12/
    /// kind@16/idle_next@17/_pad@18（自 kernel 迁移零漂——数据域偏移与
    /// 迁移前隐式尾垫形态逐字节同位）。
    #[test]
    fn switch_rec_shape_pinned() {
        assert_eq!(core::mem::size_of::<SwitchRec>(), 24, "24B 冻结面（挂账 1 断言）");
        assert_eq!(core::mem::align_of::<SwitchRec>(), 8);
        assert_eq!(core::mem::offset_of!(SwitchRec, vct), 0);
        assert_eq!(core::mem::offset_of!(SwitchRec, prev), 8);
        assert_eq!(core::mem::offset_of!(SwitchRec, next), 12);
        assert_eq!(core::mem::offset_of!(SwitchRec, kind), 16);
        assert_eq!(core::mem::offset_of!(SwitchRec, idle_next), 17);
        assert_eq!(core::mem::offset_of!(SwitchRec, _pad), 18);
    }

    /// WakeRec 形状钉板：size == 32、七字段偏移字面（迁移字面零改）。
    #[test]
    fn wake_rec_shape_pinned() {
        assert_eq!(core::mem::size_of::<WakeRec>(), 32);
        assert_eq!(core::mem::align_of::<WakeRec>(), 8);
        assert_eq!(core::mem::offset_of!(WakeRec, vct), 0);
        assert_eq!(core::mem::offset_of!(WakeRec, since_vct), 8);
        assert_eq!(core::mem::offset_of!(WakeRec, wait_obj), 16);
        assert_eq!(core::mem::offset_of!(WakeRec, task_id), 24);
        assert_eq!(core::mem::offset_of!(WakeRec, reason), 28);
        assert_eq!(core::mem::offset_of!(WakeRec, source), 29);
        assert_eq!(core::mem::offset_of!(WakeRec, _pad), 30);
    }

    /// 头/段形状钉板：32B/24B、字段偏移字面（version@0/hdr_bytes@4/
    /// cntfrq@8/ring_len@16/ring_cpus@24；family@0/cpu@4/total@8/count@16/
    /// rec_size@20——插字段/错序即红）。
    #[test]
    fn rings_hdr_seg_shape_pinned() {
        assert_eq!(core::mem::size_of::<RingsHdrV1>(), 32);
        assert_eq!(core::mem::align_of::<RingsHdrV1>(), 8);
        assert_eq!(core::mem::offset_of!(RingsHdrV1, version), 0);
        assert_eq!(core::mem::offset_of!(RingsHdrV1, hdr_bytes), 4);
        assert_eq!(core::mem::offset_of!(RingsHdrV1, cntfrq), 8);
        assert_eq!(core::mem::offset_of!(RingsHdrV1, ring_len), 16);
        assert_eq!(core::mem::offset_of!(RingsHdrV1, ring_cpus), 24);

        assert_eq!(core::mem::size_of::<RingSegV1>(), 24);
        assert_eq!(core::mem::align_of::<RingSegV1>(), 8);
        assert_eq!(core::mem::offset_of!(RingSegV1, family), 0);
        assert_eq!(core::mem::offset_of!(RingSegV1, cpu), 4);
        assert_eq!(core::mem::offset_of!(RingSegV1, total), 8);
        assert_eq!(core::mem::offset_of!(RingSegV1, count), 16);
        assert_eq!(core::mem::offset_of!(RingSegV1, rec_size), 20);
    }

    /// family 常量互异（段序判族依据——撞号即族归属撕裂）。
    #[test]
    fn ring_family_constants_distinct() {
        assert_eq!(RING_FAMILY_SWITCH, 0);
        assert_eq!(RING_FAMILY_WAKE, 1);
        assert_ne!(RING_FAMILY_SWITCH, RING_FAMILY_WAKE);
    }

    /// rec_size 域恒等：24/32 == size_of::<SwitchRec>/::<WakeRec>（内核
    /// 装配写进段描述符的 rec_size 与条目形状恒等——漂移即消费方步进错
    /// 位）。
    #[test]
    fn ring_seg_rec_size_identity() {
        assert_eq!(core::mem::size_of::<SwitchRec>(), 24);
        assert_eq!(core::mem::size_of::<WakeRec>(), 32);
        // V1 几何常量钉板（kernel cross-const assert 同值）。
        assert_eq!(TELEMETRY_RINGS_V1_RING_LEN, 64);
        assert_eq!(TELEMETRY_RINGS_V1_CPUS, 2);
        // 满载上界字面：32 + 4×24 + 2×64×24 + 2×64×32。
        assert_eq!(
            TELEMETRY_RINGS_MAX,
            32 + 4 * 24 + 2 * 64 * core::mem::size_of::<SwitchRec>()
                + 2 * 64 * core::mem::size_of::<WakeRec>()
        );
        assert_eq!(TELEMETRY_RINGS_MAX, 7296);
    }

    /// 头/段 LE 编解码 round-trip（kernel 装配 ↔ berryctl 解析同一具
    /// ——中间字节流的字段序/字节序即 ABI 面）。
    #[test]
    fn rings_hdr_seg_codec_roundtrip() {
        let h = RingsHdrV1 {
            version: RINGS_SNAP_VER,
            hdr_bytes: 32,
            cntfrq: 0x0123_4567_89ab_cdef,
            ring_len: 64,
            ring_cpus: 2,
        };
        let mut buf = [0u8; 32];
        rings_hdr_write(&h, &mut buf);
        // 字节序抽查（LE：低字节在前）。
        assert_eq!(buf[0], 1, "version=1 的 LE 首字节");
        assert_eq!(buf[4], 32, "hdr_bytes=32 的 LE 首字节");
        assert_eq!(buf[8], 0xef, "cntfrq 低字节在前（LE）");
        assert_eq!(rings_hdr_read(&buf), h, "头 round-trip 恒等");

        let s = RingSegV1 {
            family: RING_FAMILY_WAKE,
            cpu: 1,
            total: 0xfeed_face_dead_beef,
            count: 64,
            rec_size: 32,
        };
        let mut b2 = [0u8; 24];
        ring_seg_write(&s, &mut b2);
        assert_eq!(b2[0], RING_FAMILY_WAKE as u8);
        assert_eq!(b2[8], 0xef, "total 低字节在前（LE）");
        assert_eq!(ring_seg_read(&b2), s, "段 round-trip 恒等");

        // 空段 round-trip（total=0/count=0 形态——trace_off 空段/降级段的
        // 编码面：rec_size 自述恒在场）。
        let z = RingSegV1 { family: 0, cpu: 0, total: 0, count: 0, rec_size: 24 };
        let mut b3 = [0u8; 24];
        ring_seg_write(&z, &mut b3);
        assert_eq!(b3[0..20], [0u8; 20], "空段的 family/cpu/total/count 全零");
        assert_eq!(&b3[20..24], &24u32.to_le_bytes(), "rec_size 自述恒在场");
        assert_eq!(ring_seg_read(&b3), z);
    }

    // ---- op=3 WakeSrc 槽表纯镜像：形状/常量/编解码钉板（槽表读口刀 F6）----

    /// 行形状钉板：size == 64、align == 8、十字段偏移字面（wakes@0/irq@8/
    /// nw@16/first@24/last@32/ack@40/id@48/flags@52/ts_domain@54/_pad@55——
    /// kernel 原子壳 cross-const assert 同位锁步；插字段/错序即红）。
    #[test]
    fn wsrc_row_shape_pinned() {
        assert_eq!(core::mem::size_of::<WakeSrcRecV1>(), 64, "64B 冻结面");
        assert_eq!(core::mem::align_of::<WakeSrcRecV1>(), 8);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, wakes), 0);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, irq_count), 8);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, no_waiter), 16);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, first_ts), 24);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, last_ts), 32);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, last_ack_ts), 40);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, last_woken_id), 48);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, flags), 52);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, ts_domain), 54);
        assert_eq!(core::mem::offset_of!(WakeSrcRecV1, _pad), 55);
    }

    /// 头形状钉板：size == 40、七域偏移字面（version@0/hdr_bytes@4/slots@8/
    /// rec_size@12/cntfrq@16/now_vct@24/wake_acct_n@32——插字段/错序即红）。
    #[test]
    fn wsrc_hdr_shape_pinned() {
        assert_eq!(core::mem::size_of::<WsrcHdrV1>(), 40, "40B 冻结面");
        assert_eq!(core::mem::align_of::<WsrcHdrV1>(), 8);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, version), 0);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, hdr_bytes), 4);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, slots), 8);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, rec_size), 12);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, cntfrq), 16);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, now_vct), 24);
        assert_eq!(core::mem::offset_of!(WsrcHdrV1, wake_acct_n), 32);
    }

    /// 头/行 LE 编解码 round-trip（kernel 装配 ↔ berryctl powertop 解析
    /// 同一具——中间字节流的字段序/字节序即 ABI 面）＋垫区写零规范化面。
    #[test]
    fn wsrc_hdr_row_codec_roundtrip() {
        let h = WsrcHdrV1 {
            version: WSRC_SNAP_VER,
            hdr_bytes: 40,
            slots: 128,
            rec_size: 64,
            cntfrq: 0x0123_4567_89ab_cdef,
            now_vct: 0xdead_beef_cafe_1234,
            wake_acct_n: 7,
        };
        let mut buf = [0u8; 40];
        wsrc_hdr_write(&h, &mut buf);
        // 字节序抽查（LE：低字节在前）。
        assert_eq!(buf[0], 1, "version=1 的 LE 首字节");
        assert_eq!(buf[4], 40, "hdr_bytes=40 的 LE 首字节");
        assert_eq!(buf[16], 0xef, "cntfrq 低字节在前（LE）");
        assert_eq!(buf[24], 0x34, "now_vct 低字节在前（LE）");
        assert_eq!(wsrc_hdr_read(&buf), h, "头 round-trip 恒等");

        let r = WakeSrcRecV1 {
            wakes: 0xfeed_face_dead_f00d,
            irq_count: 9,
            no_waiter: 3,
            first_ts: 1_000,
            last_ts: 2_000,
            last_ack_ts: 1_900,
            last_woken_id: 0xDEAD_BEEF,
            flags: 0xBEEF,
            ts_domain: 0,
            _pad: [0; 5],
        };
        let mut rbuf = [0u8; 64];
        wsrc_row_write(&r, &mut rbuf);
        assert_eq!(rbuf[0], 0x0d, "wakes 低字节在前（LE）");
        assert_eq!(&rbuf[55..64], &[0u8; 9], "垫区写零（_pad＋repr(C) 尾垫）");
        assert_eq!(wsrc_row_read(&rbuf), r, "行 round-trip 恒等（垫零保真）");

        // 垫区写零是编码侧规范化（非仅入参纪律）：非零垫入参也落全零垫，
        // 读方不验照拷——写方恒零、读方不警察的对称契约。
        let mut dirty = r;
        dirty._pad = [1, 2, 3, 4, 5];
        let mut dbuf = [0u8; 64];
        wsrc_row_write(&dirty, &mut dbuf);
        assert_eq!(&dbuf[55..64], &[0u8; 9], "编码规范化：垫区恒落零");
    }

    /// 哨兵保真：未触行（first_ts = u64::MAX）round-trip 后哨兵原样——
    /// 消费方「未触槽可判」判据与 irq-only 槽形（merge_hal 三槽语义钉板）
    /// 的编码面。
    #[test]
    fn wsrc_row_sentinel_fidelity() {
        let r = WakeSrcRecV1 {
            wakes: 0,
            irq_count: 4,
            no_waiter: 0,
            first_ts: u64::MAX,
            last_ts: 0,
            last_ack_ts: 0,
            last_woken_id: 0,
            flags: 0,
            ts_domain: 0,
            _pad: [0; 5],
        };
        let mut buf = [0u8; 64];
        wsrc_row_write(&r, &mut buf);
        // 哨兵字节面抽查（FF ×8 @24..32）＋ round-trip 保真。
        assert_eq!(&buf[24..32], &[0xffu8; 8], "哨兵 u64::MAX 的字节面");
        let back = wsrc_row_read(&buf);
        assert_eq!(back.first_ts, u64::MAX, "未触行 round-trip 后哨兵原样");
        assert_eq!(back, r);
    }

    /// 满载上界算术恒等＋几何常量钉板：40 头 + slots×rec == 8232 字面冻结
    ///（kernel cross-const assert 同值）；rec_size 域 == size_of::<行> 恒等
    ///（漂移即消费方步进错位）。
    #[test]
    fn wsrc_max_and_geometry_identities() {
        assert_eq!(WSRC_TABLE_V1_SLOTS, 128);
        assert_eq!(WSRC_TABLE_V1_REC, 64);
        assert_eq!(core::mem::size_of::<WakeSrcRecV1>(), WSRC_TABLE_V1_REC);
        assert_eq!(core::mem::size_of::<WsrcHdrV1>(), 40);
        assert_eq!(
            TELEMETRY_WSRC_MAX,
            core::mem::size_of::<WsrcHdrV1>() + WSRC_TABLE_V1_SLOTS * WSRC_TABLE_V1_REC
        );
        assert_eq!(TELEMETRY_WSRC_MAX, 8232);
        // 头 slots/rec_size 自述域与几何常量恒等（内核装配写进头的两域）。
        let h = WsrcHdrV1 {
            version: WSRC_SNAP_VER,
            hdr_bytes: 40,
            slots: WSRC_TABLE_V1_SLOTS as u32,
            rec_size: WSRC_TABLE_V1_REC as u32,
            cntfrq: 0,
            now_vct: 0,
            wake_acct_n: 0,
        };
        assert_eq!(h.slots as usize, WSRC_TABLE_V1_SLOTS);
        assert_eq!(h.rec_size as usize, WSRC_TABLE_V1_REC);
    }
}
