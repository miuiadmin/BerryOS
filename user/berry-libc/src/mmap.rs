//! mmap 匿名档真身（A7-1 —— docs/事件循环A7设计草案.md §3.1）
//!
//! 形态：`mmap(NULL/ignored-hint, len, PROT_READ|PROT_WRITE[|PROT_EXEC],
//! MAP_ANONYMOUS|MAP_PRIVATE, -1, 0)` → **参数分诊**（[`berry_mmap::classify`]
//! 纯函数——全拒位 host 全矩阵值级断言，本侧零内联判断）→ futex 临界区
//! 内：region 表满前置拒 → **first-fit 洞分配**（窗 `[0x5000_0000,
//! 0x6000_0000)` 底起、munmap 空洞可复用——A7-0 §3.1 钦定段内分配策略）
//! → `berry_vmo_create(取整长)` + `berry_vmo_commit` 整段（内核逐页出页
//! 清零——「逐页 commit」C-2c 纪律的单 syscall 表达，pthread.rs 栈同款；
//! 物理不连续无碍）→ `berry_vmar_map(self_vmar_h(), …)`（rights 恰
//! READ|WRITE，v1 唯一档）→ 入 region 表 → 返 VA。`munmap` → **先
//! `berry_vmar_unmap` 再 close vmo**（映射中 VMO 的销毁性 close 拒
//! -EBUSY——3-2 Gotcha）+ 出表。
//!
//! **零 handle 泄漏纪律**：`vmo_create` 之后的任何失败分支，先 unmap
//! （若已映）再 close——map 失败语义 = 内核无残留翻译（未映射故 close
//! 无 -EBUSY 面）；`va != 请求位`/入表失败两分支按已映口径完整回滚
//! （结构性不可达的防御面）。first-fit 分配不消费游标（无 bump 计数
//! 器）——失败路径不耗窗，重复失败零 VA 损耗。
//!
//! **多线程口径（A7-1 修复刀）**：region 表访问全在 `MMAP_LOCK` futex
//! 临界区内（alloc.rs `HEAP_LOCK` 同款先例——钦定「futex 互斥」落地）：
//! mmap 的 [is_full/first_fit → 三 syscall → insert] 是一整段临界区
//! （防双线程同洞竞速与 is_full/insert TOCTOU）；munmap 的 take_exact /
//! mprotect 的 find 同锁。持锁期间只走 berry-sys 裸 syscall + 纯内存
//! 操作（不 malloc、不嵌套 libc 锁——与 HEAP_LOCK 无嵌套面，锁序无
//! 死锁）；表出表后的 unmap/close 内核操作在锁外（条目已独占摘除，
//! 他线程不可见）。
//!
//! ## v1 范围盒（诚实拒档，全部文档化 + host/guest 双侧钉板）
//! - mmap → `EINVAL` + `MAP_FAILED`：length==0 / 页取整溢出 / prot 缺
//!   READ 或 WRITE、含未知位（vmar_map v1 只支持恰 RW——单 READ/
//!   PROT_NONE 诚实拒）/ flags 含未知位、`MAP_SHARED`、`MAP_FIXED`、
//!   缺 `MAP_ANONYMOUS|MAP_PRIVATE` / fd≠-1 / offset≠0（文件映射 =
//!   A7-3 staging 另裁；demand paging = v2）。
//!   **忽略档（钦定裁量，非拒）**：addr≠NULL 无 MAP_FIXED = hint，
//!   POSIX 允许忽略 → 恒走窗自动分配（guest 钉板：hint 请求成功且
//!   落窗内）；`PROT_EXEC` 位忽略（用户页本可执行，NX 留 v2——rights
//!   仍传 RW）。
//! - mmap → `ENOMEM`：取整长 > 32MiB（内核 `VMO_MAX_SIZE` 同值镜像）/
//!   region 表满（16 槽——零 syscall 先拒）/ VA 窗内无整洞（first-fit
//!   窗尽）/ vmo/commit/map 内核失败。
//! - mmap → `EACCES`：gift[3] 未交付（老 spawn 面无自 vmar——能力缺席，
//!   pthread 守门同型；其用 EAGAIN 是 spawn 语义，mmap 面用 EACCES）。
//! - munmap → `EINVAL`：addr 非页对齐 / length==0 / 溢出 / 部分 unmap
//!   （基址命中但长度不符——v1 无区间拆分）；**未映射区间 → 0**（POSIX
//!   语义：非错误——Linux/musl 实测同形，与 A7-0 草案原文「未命中
//!   EINVAL」的偏差经 A7-1 审查追认取 POSIX 档，guest 钉板）；成功 → 0。
//! - mprotect（A7-1 修复刀落地钦定处置）：addr 非页对齐/len==0 →
//!   `EINVAL`；**region 命中 → `EACCES`**（诚实档不支持改权——内核
//!   `berry_vmar_protect` v1 仅接受恰 READ|WRITE = RW→RW 空转，「用之」
//!   无实义，kernel/syscall.rs sys_vmar_protect rights≠RW → -ENOTSUP）；
//!   **未命中 → `EINVAL`**。真身（unmap+remap 保内容或 protect 扩档）
//!   随 A7-3（文件映射刀）再裁。madvise 维持 stubs.rs ok! 桶（建议性
//!   接口，谎报成功 =「建议被忽略」合法档）。

use core::sync::atomic::AtomicU32;

use berry_mmap::{
    classify, round_up_page, Region, RegionTable, TakeOutcome, Reject, PAGE,
};
use berry_sys::{
    berry_handle_close, berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create,
    Handle, RIGHT_READ, RIGHT_WRITE,
};

use crate::io::self_vmar_h;
use crate::{c_int, c_void, off_t, set_errno, EACCES, EINVAL, ENOMEM};

// —— libc 契约常量单源转发（musl aarch64 冻结值，berry-mmap 为源）——
pub use berry_mmap::{
    MAP_ANON, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_EXEC, PROT_NONE, PROT_READ,
    PROT_WRITE, VMO_MAX,
};
/// 失败哨兵（usize::MAX 全 1——libc crate 同形；berry-mmap 存 usize、
/// libc 面 cast 指针）。
pub const MAP_FAILED: *mut c_void = berry_mmap::MAP_FAILED as *mut c_void;

/// region 表互斥（futex 化：0 = 空、1 = 持有；alloc.rs `HEAP_LOCK` 同款
/// 先例——A7-1 修复刀按钦定落地）。**不可重入**——临界区内不嵌套
/// libc 锁/malloc。
static MMAP_LOCK: AtomicU32 = AtomicU32::new(0);

// SAFETY: 访问全在 MMAP_LOCK futex 临界区内（mmap/munmap/mprotect 三
// 入口 lock/unlock 包络——多线程数据竞态面关闭）；表算术本体在
// berry-mmap 纯面（host 测试覆盖）。
static mut REGIONS: RegionTable = RegionTable::new();

/// mmap 主干（须持 MMAP_LOCK 调用——first_fit→insert 临界区不可分）。
/// 返 Ok(base) 或 Err(errno)。
unsafe fn mmap_locked(hv: u32, len: usize) -> Result<usize, c_int> {
    // SAFETY: 见 REGIONS 注。
    let reg = unsafe { &mut *core::ptr::addr_of_mut!(REGIONS) };
    if reg.is_full() {
        return Err(ENOMEM); // 表满前置拒：零 syscall、零内核对象诞生
    }
    let Some(base) = reg.first_fit(len) else {
        return Err(ENOMEM); // 窗内无整洞（first-fit 窗尽）
    };
    // —— vmo → commit → map（失败即回滚，零 handle 泄漏）——
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        return Err(ENOMEM);
    }
    // 整段 commit：内核逐页出页清零（C-2c「逐页 VMO commit」纪律的单
    // syscall 表达；物理不连续无碍——pthread.rs 栈同款先例）。
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        let _ = berry_handle_close(vmo); // 未映射——close 无 -EBUSY 面
        return Err(ENOMEM);
    }
    let mut va = base; // in-out 槽：请求位显式预写（v1 无重排口径）
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 {
        let _ = berry_handle_close(vmo); // 失败 = 无残留翻译（内核回滚叶）
        return Err(ENOMEM);
    }
    if va != base {
        // 结构性不可达（v1 无重排：回写恒 == 请求位）——防御回滚按已映口径。
        let _ = berry_vmar_unmap(hv, va, len);
        let _ = berry_handle_close(vmo);
        return Err(ENOMEM);
    }
    if !reg.insert(Region { base, len, vmo: vmo as usize }) {
        // is_full 前置已拦——结构性不可达的防御回滚（零 handle 泄漏）。
        let _ = berry_vmar_unmap(hv, base, len);
        let _ = berry_handle_close(vmo);
        return Err(ENOMEM);
    }
    Ok(base)
}

/// 匿名档 mmap 真身（A7-1；原 stubs.rs ENOMEM 空桩退役）。
#[no_mangle]
pub extern "C" fn mmap(
    addr: *mut c_void,
    length: usize,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: off_t,
) -> *mut c_void {
    let _ = addr; // hint 恒忽略（钦定裁量：POSIX 允许；MAP_FIXED 在 flags 面拒）
    // —— 参数分诊：全部前置判断在 berry-mmap 纯函数（host 全矩阵）——
    let len = match classify(length, prot, flags, fd, offset) {
        Ok(l) => l,
        Err(Reject::Invalid) => {
            set_errno(EINVAL);
            return MAP_FAILED;
        }
        Err(Reject::TooLarge) => {
            set_errno(ENOMEM); // 取整长超 VMO_MAX（内核帽前置镜像）
            return MAP_FAILED;
        }
    };
    let hv = self_vmar_h();
    if hv == 0 {
        set_errno(EACCES); // 老 spawn 面无自 vmar gift（能力缺席）
        return MAP_FAILED;
    }
    crate::futex::mutex_lock(&MMAP_LOCK);
    // SAFETY: 见 REGIONS 注（持锁临界区）。
    let r = unsafe { mmap_locked(hv, len) };
    crate::futex::mutex_unlock(&MMAP_LOCK);
    match r {
        Ok(base) => base as *mut c_void,
        Err(e) => {
            set_errno(e);
            MAP_FAILED
        }
    }
}

/// munmap 真身（原 stubs.rs ok! 谎报桶退役——A7-1 起有真语义可释放）。
#[no_mangle]
pub extern "C" fn munmap(addr: *mut c_void, length: usize) -> c_int {
    let a = addr as usize;
    if a % PAGE != 0 {
        set_errno(EINVAL); // POSIX：addr 须页对齐
        return -1;
    }
    let Some(len) = round_up_page(length) else {
        set_errno(EINVAL); // length==0（POSIX）/溢出
        return -1;
    };
    crate::futex::mutex_lock(&MMAP_LOCK);
    // SAFETY: 见 REGIONS 注（持锁临界区）。
    let outcome = unsafe { (*core::ptr::addr_of_mut!(REGIONS)).take_exact(a, len) };
    crate::futex::mutex_unlock(&MMAP_LOCK);
    match outcome {
        TakeOutcome::Exact(r) => {
            // 条目已独占摘除——unmap/close 在锁外（他线程不可见，无竞面）。
            let hv = self_vmar_h();
            if hv != 0 {
                // 先 unmap 再 close：映射中 VMO 的销毁性 close 拒 -EBUSY
                //（3-2 Gotcha——序即正确性）。hv==0 且表有货 = 结构性
                // 不可达（mmap 守门在先），防御跳过 unmap。
                let _ = berry_vmar_unmap(hv, r.base, r.len);
            }
            let _ = berry_handle_close(r.vmo as Handle);
            0
        }
        TakeOutcome::Partial => {
            set_errno(EINVAL); // 部分 unmap：v1 无区间拆分（诚实拒）
            -1
        }
        TakeOutcome::NotFound => 0, // POSIX：未映射区间非错误（guest 钉板）
    }
}

/// mprotect 分诊真身（A7-1 修复刀：钦定处置落地——stubs.rs 零参 EINVAL
/// 桩退役，签名对真原型 3 参定形）。v1 无改权档（详头注）——errno 契约：
/// 非页对齐/len==0 → EINVAL；region 命中（基址精确，munmap 同口径）→
/// EACCES；未命中 → EINVAL。`prot` 值不参与分诊（无改权档下任何请求
/// 同拒 EACCES——最小处置，扩档随 A7-3）。
#[no_mangle]
pub extern "C" fn mprotect(addr: *mut c_void, len: usize, _prot: c_int) -> c_int {
    let a = addr as usize;
    if a % PAGE != 0 || len == 0 {
        set_errno(EINVAL);
        return -1;
    }
    crate::futex::mutex_lock(&MMAP_LOCK);
    // SAFETY: 见 REGIONS 注（持锁临界区）。
    let hit = unsafe { (*core::ptr::addr_of_mut!(REGIONS)).find(a) }.is_some();
    crate::futex::mutex_unlock(&MMAP_LOCK);
    if hit {
        set_errno(EACCES); // 在册映射：诚实档不支持改权
    } else {
        set_errno(EINVAL); // 非在册基址
    }
    -1
}
