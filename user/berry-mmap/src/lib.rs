//! # berry-mmap —— mmap 簿记纯函数面（A7-1 匿名档 / A7-3 文件档）
//!
//! ## Overview (for crates.io users)
//!
//! `mmap` bookkeeping pure functions for the
//! [BerryOS](https://github.com/miuiadmin/BerryOS) microkernel: a
//! fixed-capacity region table, a VA-window **first-fit** allocator (hole
//! reuse after `munmap`), an argument classifier (anonymous arm since
//! A7-1, file-staging arm since A7-3), `mprotect` dormant bookkeeping,
//! and page-rounding arithmetic — pure functions, host-testable. The
//! consumer is berry-libc
//! (`mmap.rs` syscall bridge: vmo_create + vmar_map); this crate holds no
//! syscall face and no arch-specific code. no_std core; builds and
//! unit-tests on any host.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! userland ABI. Repo docs are mostly zh-CN.
//!
//! ## mmap 专属 VA 窗（A7-1 定谳）
//!
//! 窗定值 `[WIN_BASE, WIN_END)` = `[0x5000_0000, 0x6000_0000)` 256MiB。
//! **段内分配策略（A7-0 §3.1 钦定）：first-fit 底起、munmap 出表即生
//! 空洞、空洞可复用；不做合并/分裂**（洞由表项边界天然形成，无需
//! 维护）。窗内无整洞 = `None`（调用方译 ENOMEM）。**避让集实证**
//! （2026-10-30 实测）：
//! - ELF 映像 `[0x1000, base+len)`（link-user.ld 基址 0x1000；rootfs 单
//!   ELF 最大数 MiB 量级）+ 主栈紧随其后隔一页保护页
//!   （berry-exec `stack_va = base+len+PAGE`）——全部 < 0x100_0000 量级；
//! - malloc 场地 = berry-libc alloc.rs 映像 `.bss` 静态 1MiB 堆（映像
//!   跨度内，不另占 VA 段）；
//! - pthread 栈 bump 段 `[0x6000_0000, 0x7000_0000)`（pthread.rs
//!   STACK_VA_BASE——窗**右端恰贴其左端**，半开区间相邻不重叠）；
//! - MMIO 设备窗 `0x7000_0000`（VIRTIO_MMIO_VA）/ fb 窗 `0x7100_0000`
//!   （FB_VA）——pthread 段之上；
//! - 内核 root Vmar 辖域 `[0x1000, 1<<39)`——窗全在辖内；
//! - `grep 0x5000_0000` 于 user/ kernel/ drivers/ 零命中（virgin 段）。
//!
//! 窗界与 pthread 段基以 const assert 对字面值钉板（跨 crate 无依赖
//! 不能共享常量——`PTHREAD_STACK_VA_BASE_MIRROR` 镜像单源注记：两值
//! 若漂移，本 const assert 即编译期红）。
//!
//! ## region 表（AliasTable 先例）
//!
//! 定容 16 槽 `Option<Region>` 数组、调用方持存储（libc 侧 static，
//! **futex 临界区内访问**——libc 侧纪律见 berry-libc mmap.rs 头注）。
//! v1 语义：插入首空槽（槽位可复用——出表后槽还给插入面）；查表/出表
//! 按**基址精确命中**（部分 unmap 无区间拆分，v1 档 = 基址命中但长度
//! 不符，或请求区间与活映射**交叠**而非基址精确（内部地址/覆盖式跨
//! 基址）→ `TakeOutcome::Partial`，调用方译 EINVAL——杜绝「交叠但非
//! 基址 → NotFound → 0」的静默假成功）；未命中 →
//! `NotFound`（POSIX：munmap 未映射区间返 0 非错误——调用方钉板语义，
//! 与 A7-0 草案原文「未命中 EINVAL」的偏差经 A7-1 审查追认：实现取
//! POSIX 真语义档）。表容量满 = 插入面 false（ENOMEM）。`base` 唯一性
//! 由 first-fit 分配的**洞互斥性**按构造保证（在册区间两两不叠，同
//! base 二次插入不可达）。

#![cfg_attr(not(test), no_std)]

/// 页大小（与内核 PAGE_SIZE / berry-loader 同值；窗/表算术的粒度基）。
pub const PAGE: usize = 4096;

/// mmap 专属 VA 窗基址（避让集实证见头注；0x5000_0000 = 1.25GiB 处，
/// 与映像带（<32MiB 量级）间距 >1.2GiB）。
pub const WIN_BASE: usize = 0x5000_0000;
/// 窗幅 256MiB（与 pthread 栈段幅同值——段带对称规划）。
pub const WIN_SPAN: usize = 256 << 20;
/// 窗尾（半开）：恰贴 pthread 栈段基 0x6000_0000。
pub const WIN_END: usize = WIN_BASE + WIN_SPAN;

/// pthread.rs `STACK_VA_BASE` 镜像（无依赖不能共享常量——**单源注记**：
/// 两值必须同步漂移；const assert 对板执法，改任一侧即编译期红）。
const PTHREAD_STACK_VA_BASE_MIRROR: usize = 0x6000_0000;

/// 内核 `VMO_MAX_SIZE` 镜像（C-2c 起 32MiB）——**单源注记**：与 kernel
/// 常量、berry-libc pthread.rs `VMO_MAX` 同值三镜像；超此 `berry_vmo_create`
/// 必拒 -ENOMEM，classify 前置拦 = 语义同、省一跳（漂移即行为面红）。
pub const VMO_MAX: usize = 32 << 20;

// —— 窗界钉板（A7 草案 §5 风险 #2「VA 段撞车」的 const assert 执法）——
const _: () = assert!(WIN_END <= PTHREAD_STACK_VA_BASE_MIRROR); // 不越 pthread 段
const _: () = assert!(WIN_BASE % PAGE == 0); // 窗基页对齐（vmar_map 口径）
const _: () = assert!(WIN_SPAN % PAGE == 0); // 窗幅页倍数
const _: () = assert!(WIN_BASE > 0x1000_0000); // 与映像/栈带（<32MiB 量级）大幅隔离
const _: () = assert!(VMO_MAX < WIN_SPAN); // 单映射帽 < 窗幅（帽内分配才不恒拒）

/// region 表容量（AliasTable 先例的定容口径；表满 = ENOMEM 诚实拒）。
pub const REGION_MAX: usize = 16;

// —— libc 契约常量（musl aarch64 冻结值；berry-libc 经 `pub use` 单源
// 转发，bsh 自验另有本地镜像——值由 ABI 契约钉死，已对 libc crate
// aarch64-musl 0.2 逐值核验）——
pub const PROT_NONE: i32 = 0;
pub const PROT_READ: i32 = 1;
pub const PROT_WRITE: i32 = 2;
pub const PROT_EXEC: i32 = 4;

pub const MAP_SHARED: i32 = 0x01;
pub const MAP_PRIVATE: i32 = 0x02;
pub const MAP_FIXED: i32 = 0x10;
/// = MAP_ANON（musl aarch64 同值双名）。
pub const MAP_ANON: i32 = 0x20;
pub const MAP_ANONYMOUS: i32 = 0x20;

/// `mmap` 失败哨兵（usize::MAX 全 1——libc crate 同形；libc 侧 cast 成
/// `*mut c_void` 对外）。
pub const MAP_FAILED: usize = usize::MAX;

/// `usize` 上取整到页倍数；`0` 与溢出返 `None`（调用方译 EINVAL——
/// POSIX：length==0 即 EINVAL；溢出 = 病态 length 同档）。
pub fn round_up_page(n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    n.checked_add(PAGE - 1).map(|v| v & !(PAGE - 1))
}

/// 参数分诊拒因（libc 侧译 errno：`Invalid`→EINVAL、`TooLarge`→ENOMEM）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reject {
    /// EINVAL 桶：参数面契约违约（各拒位见 [`classify`]）。
    Invalid,
    /// ENOMEM 桶：取整长超 [`VMO_MAX`]（内核 vmo_create 上限的前置镜像）。
    TooLarge,
}

/// classify 的返回档型：取整后长度 + 种类分流（libc 侧按此分流主管——
/// `Anon` 零填整段、`File` 走 staging 急切私有拷贝，A7-3 裁定 A 案）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapKind {
    /// 匿名档（fd==-1）：零填整段映射（A7-1 契约零漂移）。
    Anon(usize),
    /// 文件档（fd≠-1）：MAP_PRIVATE staging 伪映射——libc 侧急切全量
    /// 私有拷贝（POSIX lazy-COW 的 eager 版，语义等价；staging 主管在
    /// berry-libc mmap.rs，本 crate 只管分诊与簿记）。
    File(usize),
}

/// mmap **参数分诊**（A7-0 §3.1 钦定「mmap 流：分诊（纯函数）」的落地
/// ——本函数承载全部 EINVAL/ENOMEM 前置判断，host 全矩阵值级断言，
/// libc 侧零内联判断）。返回档型 = 取整后长度 + 种类分流（匿名/文件
/// staging——A7-3 起文件档不再一拒了之，libc 侧按 [`MapKind`] 分流）。
///
/// 共用拒位（两档皆 `Reject::Invalid`）：
/// - `length`==0 或页取整溢出（POSIX：length==0 即 EINVAL）；
/// - `prot` 含未知位（~(READ|WRITE|EXEC)）或 `flags` 含未知位。
///
/// 匿名档（`fd`==-1，A7-1 契约**零漂移**）：
/// - `prot` 非恰含 R|W——vmar_map v1 只支持恰 RW 权限（PROT_NONE/单
///   READ/单 WRITE 诚实拒）；**PROT_EXEC 位忽略**（钦定裁量：用户页
///   本可执行、NX 留 v2——R|W|EXEC 通过，rights 仍传 RW）；
/// - `MAP_SHARED` / `MAP_FIXED` / 缺 `MAP_ANONYMOUS|MAP_PRIVATE`；
/// - `offset`≠0。
///
/// 文件档（`fd`≠-1，A7-3：MAP_PRIVATE staging 伪映射）：
/// - `flags` 含 `MAP_ANONYMOUS`（POSIX 矛盾面：ANON 与 fd 互斥）/
///   `MAP_SHARED` / `MAP_FIXED`，或缺 `MAP_PRIVATE`；
/// - `prot` 缺 READ（恒 RW 硬映射：只要求含 READ，WRITE 可选——单
///   READ 请求亦按 RW 落 rights；PROT_NONE 拒）；
/// - `offset`<0 或非页对齐（staging 读源按 offset 起读，页界口径与
///   addr 同——POSIX 同拒）。
///
/// 两档共同：取整长超 [`VMO_MAX`] → `Reject::TooLarge`。
///
/// `addr` **不入参**：非 NULL hint 恒忽略走窗自动分配（钦定裁量——
/// POSIX 允许实现忽略 hint；`MAP_FIXED` 已在 flags 面拒绝，addr 无第
/// 二语义）。拒位次序与 errno 档为 v1 契约面，guest/host 双侧钉板。
pub fn classify(length: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> Result<MapKind, Reject> {
    let len = round_up_page(length).ok_or(Reject::Invalid)?;
    const PROT_KNOWN: i32 = PROT_READ | PROT_WRITE | PROT_EXEC; // =7
    const FLAG_KNOWN: i32 = MAP_SHARED | MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS;
    if prot & !PROT_KNOWN != 0 || flags & !FLAG_KNOWN != 0 {
        return Err(Reject::Invalid);
    }
    if fd != -1 {
        // —— 文件档（A7-3）：MAP_PRIVATE staging 伪映射分诊 ——
        if flags & MAP_ANONYMOUS != 0 // POSIX 矛盾面：ANON 与 fd≠-1 互斥
            || flags & (MAP_SHARED | MAP_FIXED) != 0
            || flags & MAP_PRIVATE == 0
            // 恒 RW 硬映射：只要求含 READ（单 READ 请求亦按 RW 落 rights）
            || prot & PROT_READ == 0
            // offset 负值 / 非页对齐（staging 读源起点，页界口径与 addr 同）
            || offset < 0
            || offset as u64 % PAGE as u64 != 0
        {
            return Err(Reject::Invalid);
        }
        if len > VMO_MAX {
            return Err(Reject::TooLarge);
        }
        return Ok(MapKind::File(len));
    }
    // —— 匿名档（A7-1 契约零漂移）——
    if prot & (PROT_READ | PROT_WRITE) != (PROT_READ | PROT_WRITE) {
        return Err(Reject::Invalid);
    }
    if flags & (MAP_SHARED | MAP_FIXED) != 0
        || flags & (MAP_ANONYMOUS | MAP_PRIVATE) != (MAP_ANONYMOUS | MAP_PRIVATE)
    {
        return Err(Reject::Invalid);
    }
    if offset != 0 {
        return Err(Reject::Invalid);
    }
    if len > VMO_MAX {
        return Err(Reject::TooLarge);
    }
    Ok(MapKind::Anon(len))
}

/// 一条在册映射（libc 侧持有：`vmo` 存内核 handle 的 usize 投影）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Region {
    /// 映射基址（页对齐；= vmar_map 请求位 = 实际落位——v1 无重排）。
    pub base: usize,
    /// 映射长度（页倍数；存字节非页数——v1 rights 恒 RW 无逐条差异面，
    /// 页数 = len/PAGE 派生量，A7-1 审查裁量记录）。
    pub len: usize,
    /// 背书 VMO handle（usize 投影；munmap 先 vmar_unmap 再 close）。
    pub vmo: usize,
    /// dormant 位（A7-3 mprotect PROT_NONE 档）：true = vmar 翻译已撤
    /// （地址区间不可访问——读写即 D-abort），VMO handle 滞表保内容，
    /// mprotect 复活（RW 复映）翻回 false；munmap 照常收口（close VMO）。
    /// first_fit/查表面**不看此位**——区间占用语义与活映射同（洞算术
    /// 不因 dormant 而生洞）。
    pub dormant: bool,
}

/// `take_exact` 三态（munmap 语义分流的数据面）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TakeOutcome {
    /// 基址+长度精确命中：条目已出表，调用方走 unmap+close+返 0。
    Exact(Region),
    /// 基址命中但长度不符，或请求区间与活映射交叠而非基址精确（内部
    /// 地址/覆盖式跨基址）：部分 unmap——条目保留，调用方译 EINVAL
    /// （v1 无区间拆分，诚实拒优先于 POSIX 假成功）。
    Partial,
    /// 无任何条目基址命中：调用方按 POSIX 返 0（未映射区间非错误）。
    NotFound,
}

/// 定容 region 表（容量 [`REGION_MAX`]；`AliasTable` 先例形态：
/// `const fn new` + 首空槽插入 + 精确查/出表 + first-fit 洞搜索）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionTable {
    slots: [Option<Region>; REGION_MAX],
}

impl RegionTable {
    /// 空表（libc 侧 `static mut` 初始化用——const fn 是前提）。
    pub const fn new() -> Self {
        Self {
            slots: [None; REGION_MAX],
        }
    }

    /// 在册条数。
    pub fn count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// 表满？（libc 侧在 vmo_create **之前**问——零 syscall 先拒，
    /// 表满路径零内核对象诞生 = 零 handle 泄漏面）。
    pub fn is_full(&self) -> bool {
        self.count() == REGION_MAX
    }

    /// 插入首空槽；表满 `false`。（槽位随出表可复用；`base` 唯一性由
    /// first-fit 洞互斥**按构造**保证——同 base 二次插入不可达。）
    pub fn insert(&mut self, r: Region) -> bool {
        for s in self.slots.iter_mut() {
            if s.is_none() {
                *s = Some(r);
                return true;
            }
        }
        false
    }

    /// 基址精确查表。
    pub fn find(&self, base: usize) -> Option<Region> {
        self.slots
            .iter()
            .flatten()
            .find(|r| r.base == base)
            .copied()
    }

    /// 基址精确查表（可变形态——mprotect dormant 翻转用；调用方须持
    /// 表互斥，口径同 [`find`]）。
    pub fn find_mut(&mut self, base: usize) -> Option<&mut Region> {
        self.slots.iter_mut().flatten().find(|r| r.base == base)
    }

    /// 基址精确出表三态（munmap 语义分流，见 [`TakeOutcome`]）。
    pub fn take_exact(&mut self, base: usize, len: usize) -> TakeOutcome {
        let mut overlap = false;
        for s in self.slots.iter_mut() {
            if let Some(r) = *s {
                if r.base == base {
                    return if r.len == len {
                        *s = None;
                        TakeOutcome::Exact(r)
                    } else {
                        TakeOutcome::Partial
                    };
                }
                // 区间交叠探查（半开）：addr 落活映射内部 / 请求区间覆盖
                // 跨基址——皆非 v1 可拆分形态，并入 Partial 档诚实拒
                // （若落 NotFound→0 = 静默假成功：条目滞表、映射帧滞留
                // 到进程退出，shrink 型 munmap(base+new, old-new) 即踩）。
                // saturating：纯函数不信任调用方——病态 addr+len 溢出
                // 饱和取 MAX → 判交叠（落诚实拒档）。
                if base < r.base.saturating_add(r.len) && r.base < base.saturating_add(len) {
                    overlap = true;
                }
            }
        }
        if overlap {
            TakeOutcome::Partial
        } else {
            TakeOutcome::NotFound
        }
    }

    /// **first-fit 分配基址**（钦定 §3.1 段内分配策略：底起扫、
    /// munmap 空洞可复用、无合并/分裂——洞由在册条目边界天然形成）。
    /// 窗内无整洞返 `None`（调用方译 ENOMEM）。纯函数不信任调用方：
    /// `len` 非页倍数或 0 一律 `None`。
    ///
    /// 分配与在册条目互斥由洞算术保证（只在 gap ≥ len 处落位）；调用
    /// 方须在**持有表互斥**的临界区内先 `first_fit` 后 `insert`（两步
    /// 之间夹内核 syscall——libc 侧 futex 锁包络，防双线程同洞竞速）。
    pub fn first_fit(&self, len: usize) -> Option<usize> {
        if len == 0 || len % PAGE != 0 {
            return None;
        }
        // 16 槽栈上快照 + 基址插入排序（REGION_MAX 定容小表，零分配）。
        let mut live = [(0usize, 0usize); REGION_MAX];
        let mut n = 0usize;
        for r in self.slots.iter().flatten() {
            live[n] = (r.base, r.len);
            n += 1;
        }
        for i in 1..n {
            let key = live[i];
            let mut j = i;
            while j > 0 && live[j - 1].0 > key.0 {
                live[j] = live[j - 1];
                j -= 1;
            }
            live[j] = key;
        }
        let mut cur = WIN_BASE;
        for &(base, rlen) in &live[..n] {
            if base >= cur && base - cur >= len {
                return Some(cur); // 底起首个可容洞
            }
            // 条目占据 [base, base+rlen)：游标单调推过其尾（saturating +
            // max 防御越窗/交叠条目——first_fit 产物按构造无此形态）。
            cur = base.saturating_add(rlen).max(cur);
            if cur > WIN_END {
                return None; // 表防御：条目越窗 = 无余量
            }
        }
        if WIN_END - cur >= len {
            Some(cur) // 尾洞（含空表首配 = WIN_BASE）
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— round_up_page：对齐取整 + 0/溢出拒 ————————————————
    #[test]
    fn round_up_basics() {
        assert_eq!(round_up_page(1), Some(PAGE));
        assert_eq!(round_up_page(PAGE), Some(PAGE)); // 恰页倍不动
        assert_eq!(round_up_page(PAGE + 1), Some(2 * PAGE));
        assert_eq!(round_up_page(PAGE - 1), Some(PAGE));
    }

    #[test]
    fn round_up_zero_rejected() {
        // POSIX：length==0 = EINVAL 桶（None 是其数据面）
        assert_eq!(round_up_page(0), None);
    }

    #[test]
    fn round_up_overflow_rejected() {
        assert_eq!(round_up_page(usize::MAX), None);
        assert_eq!(round_up_page(usize::MAX - 1), None);
        // 页对齐的顶值不溢出（2^64-PAGE 取整 = 自身）
        assert_eq!(round_up_page(usize::MAX - (PAGE - 1)), Some(usize::MAX - (PAGE - 1)));
    }

    // —— classify：正档矩阵（钦定「分诊（纯函数）」的全值级锚）———
    #[test]
    fn classify_ok_canonical_and_rounding() {
        // 典范调用：NULL 自动分配 + RW + ANON|PRIVATE + fd -1 + off 0
        assert_eq!(classify(1, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0), Ok(MapKind::Anon(PAGE)));
        assert_eq!(
            classify(PAGE, PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0),
            Ok(MapKind::Anon(PAGE)) // MAP_ANON 双名同值
        );
        assert_eq!(
            classify(PAGE + 1, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0),
            Ok(MapKind::Anon(2 * PAGE)) // 取整到页
        );
    }

    #[test]
    fn classify_ok_exec_ignored() {
        // 钦定裁量：PROT_EXEC 忽略（用户页本可执行，NX 留 v2）——
        // R|W|EXEC 通过（rights 仍传 RW），与恰 R|W 同档 Ok。
        assert_eq!(
            classify(PAGE, PROT_READ | PROT_WRITE | PROT_EXEC, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0),
            Ok(MapKind::Anon(PAGE))
        );
    }

    #[test]
    fn classify_ok_vmo_max_boundary() {
        // 帽界：恰 32MiB 过（内核 vmo_create 含等值档——pthread.rs 同口径）
        assert_eq!(
            classify(VMO_MAX, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0),
            Ok(MapKind::Anon(VMO_MAX))
        );
        // 帽内最大取整值：VMO_MAX-PAGE+1 → 取整 VMO_MAX 仍过
        assert_eq!(
            classify(VMO_MAX - PAGE + 1, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0),
            Ok(MapKind::Anon(VMO_MAX))
        );
    }

    #[test]
    fn classify_reject_prot_matrix() {
        for prot in [
            PROT_NONE,                                    // 0：匿名无内容映射拒
            PROT_READ,                                    // 单 READ：vmar_map v1 只支持恰 RW
            PROT_WRITE,                                   // 单 WRITE：同上
            PROT_READ | 0x8,                              // 未知 prot 位
            PROT_READ | PROT_WRITE | PROT_EXEC | 0x10,    // 已知位 + 未知位
        ] {
            assert_eq!(
                classify(PAGE, prot, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0),
                Err(Reject::Invalid),
                "prot={prot:#x}"
            );
        }
    }

    #[test]
    fn classify_reject_flags_matrix() {
        let rw = PROT_READ | PROT_WRITE;
        for flags in [
            0,                                   // 全缺
            MAP_SHARED,                          // v1 无共享档
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_SHARED, // ANON|PRIVATE 叠 SHARED
            MAP_FIXED,                           // v1 无固定地址面
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED,  // 叠 FIXED
            MAP_ANONYMOUS,                       // 缺 PRIVATE
            MAP_PRIVATE,                         // 缺 ANONYMOUS
            MAP_ANONYMOUS | MAP_PRIVATE | 0x40,  // 未知 flag 位
        ] {
            assert_eq!(
                classify(PAGE, rw, flags, -1, 0),
                Err(Reject::Invalid),
                "flags={flags:#x}"
            );
        }
    }

    #[test]
    fn classify_reject_fd_offset_length() {
        let rw = PROT_READ | PROT_WRITE;
        let fl = MAP_ANONYMOUS | MAP_PRIVATE;
        // fd≠-1 且 fl 含 ANON = 文件档矛盾面（A7-3 前此处一拒了之，
        // A7-3 起改按文件档分诊——含 ANON 仍拒，零漂移）
        assert_eq!(classify(PAGE, rw, fl, 0, 0), Err(Reject::Invalid));
        assert_eq!(classify(PAGE, rw, fl, 3, 0), Err(Reject::Invalid));
        // offset≠0（正负同拒；匿名档 offset 必须 0）
        assert_eq!(classify(PAGE, rw, fl, -1, 8), Err(Reject::Invalid));
        assert_eq!(classify(PAGE, rw, fl, -1, -4096), Err(Reject::Invalid));
        // length==0（POSIX EINVAL）与页取整溢出
        assert_eq!(classify(0, rw, fl, -1, 0), Err(Reject::Invalid));
        assert_eq!(classify(usize::MAX, rw, fl, -1, 0), Err(Reject::Invalid));
    }

    #[test]
    fn classify_too_large_over_cap() {
        let rw = PROT_READ | PROT_WRITE;
        let fl = MAP_ANONYMOUS | MAP_PRIVATE;
        // 帽外一页（取整 VMO_MAX+PAGE）→ ENOMEM 桶（vmo_create 前置镜像）
        assert_eq!(classify(VMO_MAX + 1, rw, fl, -1, 0), Err(Reject::TooLarge));
        assert_eq!(classify(VMO_MAX + PAGE, rw, fl, -1, 0), Err(Reject::TooLarge));
        assert_eq!(classify(usize::MAX - (PAGE - 1), rw, fl, -1, 0), Err(Reject::TooLarge));
    }

    // —— classify 文件档（A7-3）：正档矩阵 + 拒位矩阵 ————————————
    #[test]
    fn classify_file_ok_matrix() {
        let fl = MAP_PRIVATE;
        // 恒 RW 硬映射：恰 RW / 单 R（WRITE 可选）/ R|W|X（EXEC 忽略同匿名档）
        assert_eq!(classify(PAGE, PROT_READ | PROT_WRITE, fl, 3, 0), Ok(MapKind::File(PAGE)));
        assert_eq!(classify(PAGE, PROT_READ, fl, 3, 0), Ok(MapKind::File(PAGE)));
        assert_eq!(classify(PAGE, PROT_READ | PROT_WRITE | PROT_EXEC, fl, 3, 0), Ok(MapKind::File(PAGE)));
        // offset 页倍（0 / 4096 / 大偏移皆正档）
        assert_eq!(classify(PAGE, PROT_READ, fl, 3, 4096), Ok(MapKind::File(PAGE)));
        assert_eq!(classify(PAGE, PROT_READ, fl, 3, 0x10_0000), Ok(MapKind::File(PAGE)));
        // 取整到页（A7-1 同款：length=1 → PAGE）
        assert_eq!(classify(1, PROT_READ, fl, 3, 0), Ok(MapKind::File(PAGE)));
        assert_eq!(classify(PAGE + 1, PROT_READ, fl, 3, 0), Ok(MapKind::File(2 * PAGE)));
        // 帽界恰过（与匿名档同口径）
        assert_eq!(classify(VMO_MAX, PROT_READ, fl, 3, 0), Ok(MapKind::File(VMO_MAX)));
    }

    #[test]
    fn classify_file_reject_flags() {
        let r = PROT_READ;
        for flags in [
            MAP_ANONYMOUS,               // POSIX 矛盾面：ANON 与 fd≠-1 互斥
            MAP_ANONYMOUS | MAP_PRIVATE, // 同上（叠 PRIVATE 仍矛盾）
            0,                           // 缺 PRIVATE
            MAP_SHARED,                  // v1 无共享档
            MAP_PRIVATE | MAP_SHARED,    // 叠 SHARED
            MAP_PRIVATE | MAP_FIXED,     // v1 无固定地址面
            MAP_PRIVATE | 0x40,          // 未知 flag 位
        ] {
            assert_eq!(
                classify(PAGE, r, flags, 3, 0),
                Err(Reject::Invalid),
                "flags={flags:#x}"
            );
        }
    }

    #[test]
    fn classify_file_reject_offset_prot_len() {
        let fl = MAP_PRIVATE;
        // offset 非页对齐（正非对齐/负同拒；负值独立于对齐判拒）
        assert_eq!(classify(PAGE, PROT_READ, fl, 3, 8), Err(Reject::Invalid));
        assert_eq!(classify(PAGE, PROT_READ, fl, 3, -4096), Err(Reject::Invalid));
        // 缺 READ：PROT_NONE / 单 WRITE（恒 RW 硬映射仍要求 READ 在位）
        assert_eq!(classify(PAGE, PROT_NONE, fl, 3, 0), Err(Reject::Invalid));
        assert_eq!(classify(PAGE, PROT_WRITE, fl, 3, 0), Err(Reject::Invalid));
        // 共用面：未知 prot 位 / length==0 / 溢出
        assert_eq!(classify(PAGE, PROT_READ | 0x8, fl, 3, 0), Err(Reject::Invalid));
        assert_eq!(classify(0, PROT_READ, fl, 3, 0), Err(Reject::Invalid));
        assert_eq!(classify(usize::MAX, PROT_READ, fl, 3, 0), Err(Reject::Invalid));
    }

    #[test]
    fn classify_file_too_large() {
        // 文件档同帽：取整超 VMO_MAX → ENOMEM 桶（staging VMO 同一上限）
        assert_eq!(classify(VMO_MAX + 1, PROT_READ, MAP_PRIVATE, 3, 0), Err(Reject::TooLarge));
        assert_eq!(classify(usize::MAX - (PAGE - 1), PROT_READ, MAP_PRIVATE, 3, 0), Err(Reject::TooLarge));
    }

    // —— first_fit：底起打包 / 空洞复用 / 窗尽 ——————————————————
    #[test]
    fn first_fit_empty_table_picks_window_base() {
        let t = RegionTable::new();
        assert_eq!(t.first_fit(PAGE), Some(WIN_BASE));
        assert_eq!(t.first_fit(2 * PAGE), Some(WIN_BASE));
        assert_eq!(t.first_fit(WIN_SPAN), Some(WIN_BASE)); // 整窗一取
    }

    #[test]
    fn first_fit_packs_above_live_region() {
        // 单在册 [WIN_BASE, +2P)：新分配紧贴其顶（向上打包）
        let mut t = RegionTable::new();
        assert!(t.insert(reg(WIN_BASE, 2 * PAGE)));
        assert_eq!(t.first_fit(PAGE), Some(WIN_BASE + 2 * PAGE));
        assert_eq!(t.first_fit(2 * PAGE), Some(WIN_BASE + 2 * PAGE));
        // 3P 跳过紧邻洞位？不——底起首个 ≥len 的洞是尾洞（条目上方）
        assert_eq!(t.first_fit(3 * PAGE), Some(WIN_BASE + 2 * PAGE));
    }

    #[test]
    fn first_fit_hole_reuse_after_munmap() {
        // 钦定语义锚：q 仍活在 p1 顶上，p1 出表留底洞 → 同长再分配复用 p1
        let mut t = RegionTable::new();
        let p1 = WIN_BASE;
        let q = WIN_BASE + 2 * PAGE;
        assert!(t.insert(reg(p1, 2 * PAGE)));
        assert!(t.insert(reg(q, PAGE)));
        assert_eq!(t.take_exact(p1, 2 * PAGE), TakeOutcome::Exact(reg(p1, 2 * PAGE)));
        // 底洞恰容 2P → 复用 p1（bump 无复用形态在此必破）
        assert_eq!(t.first_fit(2 * PAGE), Some(p1));
        // 底洞不容 3P → 跳过 q 落尾洞
        assert_eq!(t.first_fit(3 * PAGE), Some(q + PAGE));
    }

    #[test]
    fn first_fit_bottom_hole_and_mid_gap() {
        // 底洞：条目悬在 WIN_BASE+2P → 底洞 [WIN_BASE, +2P) 恰容 2P
        let mut t = RegionTable::new();
        assert!(t.insert(reg(WIN_BASE + 2 * PAGE, PAGE)));
        assert_eq!(t.first_fit(2 * PAGE), Some(WIN_BASE));
        assert_eq!(t.first_fit(3 * PAGE), Some(WIN_BASE + 3 * PAGE)); // 底洞不容 → 尾洞
        // 中缝洞：两条目间 1P 缝 → 1P 分配落缝、2P 越缝落尾
        let mut t2 = RegionTable::new();
        assert!(t2.insert(reg(WIN_BASE, PAGE)));
        assert!(t2.insert(reg(WIN_BASE + 2 * PAGE, PAGE)));
        assert_eq!(t2.first_fit(PAGE), Some(WIN_BASE + PAGE));
        assert_eq!(t2.first_fit(2 * PAGE), Some(WIN_BASE + 3 * PAGE));
    }

    #[test]
    fn first_fit_insertion_order_invariant() {
        // 快照排序吸收任意插入序：同表不同插入序 → 同分配结果
        let mut a = RegionTable::new();
        assert!(a.insert(reg(WIN_BASE + 4 * PAGE, PAGE)));
        assert!(a.insert(reg(WIN_BASE, 2 * PAGE)));
        assert!(a.insert(reg(WIN_BASE + 3 * PAGE, PAGE)));
        let mut b = RegionTable::new();
        assert!(b.insert(reg(WIN_BASE, 2 * PAGE)));
        assert!(b.insert(reg(WIN_BASE + 3 * PAGE, PAGE)));
        assert!(b.insert(reg(WIN_BASE + 4 * PAGE, PAGE)));
        for len in [PAGE, 2 * PAGE, 3 * PAGE, 4 * PAGE] {
            assert_eq!(a.first_fit(len), b.first_fit(len), "len={len}");
        }
        assert_eq!(a.first_fit(PAGE), Some(WIN_BASE + 2 * PAGE)); // 中缝
    }

    #[test]
    fn first_fit_last_page_and_exhaustion() {
        // 余量恰 1P：尾洞基 = 窗尾-1页；2P 拒
        let mut t = RegionTable::new();
        assert!(t.insert(reg(WIN_BASE, WIN_SPAN - PAGE)));
        assert_eq!(t.first_fit(PAGE), Some(WIN_END - PAGE));
        assert_eq!(t.first_fit(2 * PAGE), None);
        // 整窗一取后再分配 = 窗尽
        let mut t2 = RegionTable::new();
        assert!(t2.insert(reg(WIN_BASE, WIN_SPAN)));
        assert_eq!(t2.first_fit(PAGE), None);
        // 洞幅整窗（取出后再分配恢复整窗可用——复用的极限面）
        assert_eq!(t2.take_exact(WIN_BASE, WIN_SPAN), TakeOutcome::Exact(reg(WIN_BASE, WIN_SPAN)));
        assert_eq!(t2.first_fit(WIN_SPAN), Some(WIN_BASE));
    }

    #[test]
    fn first_fit_rejects_bad_len() {
        // 纯函数不信任调用方：0 长 / 未取整长一律 None
        let t = RegionTable::new();
        assert_eq!(t.first_fit(0), None);
        assert_eq!(t.first_fit(PAGE + 1), None);
    }

    #[test]
    fn window_bounds_pinned() {
        // 避让集钉板（A7 草案 §5 #2）：窗尾恰贴 pthread 段基、不越界
        assert_eq!(WIN_END, 0x6000_0000);
        assert_eq!(WIN_BASE, 0x5000_0000);
        assert_eq!(WIN_SPAN, 256 << 20);
        assert_eq!(VMO_MAX, 32 << 20); // 内核 VMO_MAX_SIZE 镜像钉板
    }

    // —— region 表：插入/查表/容量/出表三态 ——————————————
    fn reg(base: usize, len: usize) -> Region {
        Region { base, len, vmo: 0x100 + base as usize % 97, dormant: false }
    }

    #[test]
    fn region_insert_find_hit_and_miss() {
        let mut t = RegionTable::new();
        let r = reg(0x5000_1000, PAGE);
        assert!(t.insert(r));
        assert_eq!(t.find(0x5000_1000), Some(r)); // 命中
        assert_eq!(t.find(0x5000_1000 + PAGE), None); // 基址不匹配（区间内≠命中——v1 基址精确口径）
        assert_eq!(t.find(0), None);
        assert_eq!(t.count(), 1);
    }

    #[test]
    fn region_capacity_sixteen_then_full() {
        let mut t = RegionTable::new();
        for i in 0..REGION_MAX {
            assert!(t.insert(reg(WIN_BASE + i * PAGE, PAGE)), "slot {i}");
        }
        assert!(t.is_full());
        assert!(!t.insert(reg(WIN_BASE + REGION_MAX * PAGE, PAGE))); // 17th 拒
        assert_eq!(t.count(), REGION_MAX);
    }

    #[test]
    fn region_take_exact_flow() {
        let mut t = RegionTable::new();
        let r = reg(0x5000_2000, 2 * PAGE);
        assert!(t.insert(r));
        // 精确命中：出表 + 条目消失
        assert_eq!(t.take_exact(0x5000_2000, 2 * PAGE), TakeOutcome::Exact(r));
        assert_eq!(t.find(0x5000_2000), None);
        assert_eq!(t.count(), 0);
        // 再取 = NotFound（POSIX：未映射区间 munmap 返 0 的数据面钉板）
        assert_eq!(t.take_exact(0x5000_2000, 2 * PAGE), TakeOutcome::NotFound);
    }

    #[test]
    fn region_take_partial_length_mismatch() {
        let mut t = RegionTable::new();
        let r = reg(0x5000_3000, 4 * PAGE);
        assert!(t.insert(r));
        // 基址命中但长度短于条目（部分 unmap）→ Partial，条目保留
        assert_eq!(t.take_exact(0x5000_3000, PAGE), TakeOutcome::Partial);
        // 长于条目同档 Partial（覆盖超面 v1 无拆分）
        assert_eq!(t.take_exact(0x5000_3000, 5 * PAGE), TakeOutcome::Partial);
        assert_eq!(t.find(0x5000_3000), Some(r)); // 未动
        assert_eq!(t.count(), 1);
    }

    #[test]
    fn region_take_interior_and_covering_partial() {
        // 活映射内部地址 / 覆盖式跨基址：非基址精确但区间相关 →
        // Partial 诚实拒（条目保留）——「交叠但非基址 → NotFound → 0」
        // 静默假成功面的钉板（shrink 型 munmap(base+new, old-new) 形态）。
        let mut t = RegionTable::new();
        let r = reg(0x5000_6000, 4 * PAGE);
        assert!(t.insert(r));
        assert_eq!(t.take_exact(0x5000_7000, PAGE), TakeOutcome::Partial); // 内部地址
        assert_eq!(t.take_exact(0x5000_5000, 8 * PAGE), TakeOutcome::Partial); // 覆盖式
        assert_eq!(t.take_exact(0x5000_5000, PAGE), TakeOutcome::NotFound); // 左恰邻（尾=条目基）
        assert_eq!(t.take_exact(0x5000_A000, PAGE), TakeOutcome::NotFound); // 右恰邻（基=条目尾）
        assert_eq!(t.find(0x5000_6000), Some(r)); // 条目未动
        // 随后基址精确收口仍可达（Partial 不消耗条目）
        assert_eq!(t.take_exact(0x5000_6000, 4 * PAGE), TakeOutcome::Exact(r));
        assert_eq!(t.count(), 0);
    }

    #[test]
    fn region_take_not_found_for_foreign_base() {
        let mut t = RegionTable::new();
        assert!(t.insert(reg(0x5000_4000, PAGE)));
        // 别段基址（含窗内未分配位）= NotFound；条目不动
        assert_eq!(t.take_exact(0x6000_0000, PAGE), TakeOutcome::NotFound);
        assert_eq!(t.take_exact(0x5000_5000, PAGE), TakeOutcome::NotFound);
        assert_eq!(t.count(), 1);
    }

    #[test]
    fn region_slot_reuse_after_take() {
        // 槽位复用语义（与 VA 洞复用分账）：出表后空槽可再插
        let mut t = RegionTable::new();
        for i in 0..REGION_MAX {
            assert!(t.insert(reg(WIN_BASE + i * PAGE, PAGE)));
        }
        assert_eq!(t.take_exact(WIN_BASE + 3 * PAGE, PAGE), TakeOutcome::Exact(reg(WIN_BASE + 3 * PAGE, PAGE)));
        assert!(!t.is_full());
        assert!(t.insert(reg(WIN_BASE + 3 * PAGE, PAGE))); // 复用刚空出的槽（同 base 再插 = 洞复用后合法）
        assert_eq!(t.count(), REGION_MAX);
    }

    // —— dormant 簿记（A7-3 mprotect PROT_NONE 档）————————————
    #[test]
    fn region_dormant_flip_via_find_mut() {
        // find_mut 字面往返：翻 true（unmap 撤翻译）→ find 反映 →
        // 翻 false（复映复活）→ find 回到原值（vmo/base/len 不动）。
        let mut t = RegionTable::new();
        let r = reg(WIN_BASE, 2 * PAGE);
        assert!(t.insert(r));
        t.find_mut(WIN_BASE).unwrap().dormant = true;
        let d = t.find(WIN_BASE).unwrap();
        assert!(d.dormant);
        assert_eq!((d.base, d.len, d.vmo), (r.base, r.len, r.vmo)); // 保内容字段不动
        t.find_mut(WIN_BASE).unwrap().dormant = false;
        assert_eq!(t.find(WIN_BASE), Some(r)); // 全量回原值
        // 未命中 None（与 find 同口径）
        assert!(t.find_mut(WIN_BASE + PAGE).is_none());
    }

    #[test]
    fn region_dormant_occupies_window_and_takeable() {
        // dormant 条目仍占区间：first_fit 不因 dormant 生洞（翻 true 后
        // 同窗分配落其上方）；take_exact 照常收口（munmap 语义不感知
        // dormant——libc 侧 Exact 流程 unmap 错误被忽略、close VMO 无
        // 映射无 EBUSY，纯数据面在此钉板出表可达）。
        let mut t = RegionTable::new();
        let r = reg(WIN_BASE, 2 * PAGE);
        assert!(t.insert(r));
        t.find_mut(WIN_BASE).unwrap().dormant = true;
        assert_eq!(t.first_fit(PAGE), Some(WIN_BASE + 2 * PAGE)); // 不复用 dormant 区间
        assert_eq!(t.first_fit(2 * PAGE), Some(WIN_BASE + 2 * PAGE));
        // 出表值携带 dormant 位（表内现存形态）——dormant 不拦截收口
        assert_eq!(t.take_exact(WIN_BASE, 2 * PAGE), TakeOutcome::Exact(Region { dormant: true, ..r }));
        assert_eq!(t.count(), 0);
    }

    // —— libc 桥语义自检：分配与表操作的合同面 ——————————
    #[test]
    fn alloc_then_table_two_mappings_lifecycle() {
        // 模拟 libc 侧「双映射 + 中间 munmap + 空洞复用」的簿记序（纯
        // 数据面）：first_fit 与表插入/出表互不误伤、复用语义闭环。
        let mut t = RegionTable::new();
        // p1 = 双页 @ 底；q = 单页 紧贴其顶
        let p1 = t.first_fit(2 * PAGE).unwrap();
        assert!(t.insert(Region { base: p1, len: 2 * PAGE, vmo: 0x10, dormant: false }));
        let q = t.first_fit(PAGE).unwrap();
        assert_eq!(q, p1 + 2 * PAGE); // 向上打包
        assert!(t.insert(Region { base: q, len: PAGE, vmo: 0x11, dormant: false }));
        assert_eq!(t.count(), 2);
        // p1 出表 → 底洞；q 仍活 → 同长再分配必复用 p1（钦定空洞复用）
        assert_eq!(t.take_exact(p1, 2 * PAGE), TakeOutcome::Exact(Region { base: p1, len: 2 * PAGE, vmo: 0x10, dormant: false }));
        let p2 = t.first_fit(2 * PAGE).unwrap();
        assert_eq!(p2, p1);
        assert!(t.insert(Region { base: p2, len: 2 * PAGE, vmo: 0x12, dormant: false }));
        // 先出的不是表首（防「首槽特例」侥幸）
        assert_eq!(t.take_exact(q, PAGE), TakeOutcome::Exact(Region { base: q, len: PAGE, vmo: 0x11, dormant: false }));
        assert_eq!(t.take_exact(p2, 2 * PAGE), TakeOutcome::Exact(Region { base: p2, len: 2 * PAGE, vmo: 0x12, dormant: false }));
        assert_eq!(t.count(), 0);
        assert_eq!(t.first_fit(PAGE), Some(WIN_BASE)); // 清场后回底
    }
}
