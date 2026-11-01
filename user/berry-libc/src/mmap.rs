//! mmap 真身（A7-1 匿名档 / A7-3 文件档 staging + mprotect dormant
//! —— docs/事件循环A7-3实施设计草案.md）
//!
//! 匿名档形态：`mmap(NULL/ignored-hint, len, PROT_READ|PROT_WRITE[|PROT_EXEC],
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
//! 文件档形态（A7-3，`fd`≠-1 + `MAP_PRIVATE`）：**staging 伪映射**——
//! 急切全量私有拷贝（POSIX lazy-COW 的 eager 版，语义等价；A7-3 裁定
//! A 案，B 案 kill criteria 回退档）。锁外预检（fd 快照 + STAT 定长）
//! → 锁内 `vmo_create` + `vmo_commit` 整段（**含 [length, 取整len) 尾零
//! 页**——设计稿「vmo_write 隐式 commit」在尾零页上不成立：未 commit
//! 页映射读 = D-abort；整段 commit + chunk 覆写 = 与匿名档同型零新
//! 内核行为假设）→ `get_chunk` 直 RPC 循环 `vmo_write` 覆写 [0, length)
//! （不经 fs.rs RA 缓冲——fd 游标/RA 状态零扰动；`rpc()` 零锁实证，
//! MMAP_LOCK 临界区内走无锁序嵌套面）→ `vmar_map` → 入表。
//!
//! **零 handle 泄漏纪律**：`vmo_create` 之后的任何失败分支，先 unmap
//! （若已映）再 close——map 失败语义 = 内核无残留翻译（未映射故 close
//! 无 -EBUSY 面）；`va != 请求位`/入表失败两分支按已映口径完整回滚
//! （结构性不可达的防御面）；staging 半途失败按未映口径 close。first-fit
//! 分配不消费游标（无 bump 计数器）——失败路径不耗窗，重复失败零 VA
//! 损耗。
//!
//! **多线程口径（A7-1 修复刀）**：region 表访问全在 `MMAP_LOCK` futex
//! 临界区内（alloc.rs `HEAP_LOCK` 同款先例——钦定「futex 互斥」落地）：
//! mmap 的 [is_full/first_fit → 三 syscall → insert] 是一整段临界区
//! （防双线程同洞竞速与 is_full/insert TOCTOU）；munmap 的 take_exact /
//! mprotect 的 find 同锁。持锁期间只走 berry-sys 裸 syscall + berry-fsclient
//! 直 RPC（零 libc 锁、零 malloc——与 HEAP_LOCK 无嵌套面，锁序无死锁）；
//! 表出表后的 unmap/close 内核操作在锁外（条目已独占摘除，他线程不可见）。
//!
//! ## v1 范围盒（诚实拒档，全部文档化 + host/guest 双侧钉板）
//! - mmap → `EINVAL` + `MAP_FAILED`：length==0 / 页取整溢出 / prot 含
//!   未知位，或匿名档缺 READ/WRITE、文件档缺 READ（两档分别见
//!   berry-mmap classify 拒位清单）/ flags 含未知位、`MAP_SHARED`、
//!   `MAP_FIXED`、匿名档缺 `MAP_ANONYMOUS|MAP_PRIVATE`、文件档缺
//!   `MAP_PRIVATE` 或叠 `MAP_ANONYMOUS`（POSIX 矛盾面）/ 匿名档
//!   offset≠0、文件档 offset<0 或非页对齐 / 文件档 `offset+length`
//!   超 EOF（**原始 length 判**——取整 len 判会把非页倍文件结构性变
//!   不可映射）。
//!   **忽略档（钦定裁量，非拒）**：addr≠NULL 无 MAP_FIXED = hint，
//!   POSIX 允许忽略 → 恒走窗自动分配（guest 钉板：hint 请求成功且
//!   落窗内）；`PROT_EXEC` 位忽略（用户页本可执行，NX 留 v2——rights
//!   仍传 RW）。
//! - mmap → `ENOMEM`：取整长 > 32MiB（内核 `VMO_MAX_SIZE` 同值镜像）/
//!   region 表满（16 槽——零 syscall 先拒）/ VA 窗内无整洞（first-fit
//!   窗尽）/ vmo/commit/map 内核失败/mprotect dormant 复映失败（维持
//!   dormant 零泄漏——VMO 滞表可 munmap 收口）。
//! - mmap → `EACCES`：gift[3] 未交付（老 spawn 面无自 vmar——能力缺席，
//!   pthread 守门同型；其用 EAGAIN 是 spawn 语义，mmap 面用 EACCES）/
//!   **文件档 fd 非 O_RDONLY**（O_RDWR/O_WRONLY fd 语义上是共享档的
//!   面——v1 无 MAP_SHARED 档，同拒 EACCES）/ mprotect 无改权档组合
//!   （见下）。
//! - mmap → `EBADF`：文件档 fd<3（console 段）/ fd 表查无。
//! - mmap → `EISDIR`：文件档 fd 指向目录（open 段同口径）。
//! - **文件档 POSIX 偏差清单（A7-3 §3.3 文档化）**：MAP_PRIVATE 恒
//!   急切全量拷贝（≤32MiB staging 双倍内存瞬时档）；恒 RW 硬映射
//!   （PROT_READ 请求亦 RW 落 rights——内核无只读映射档）；只收恰
//!   O_RDONLY fd；MAP_SHARED→EINVAL；offset+len ≤ file_size 强制。
//! - munmap → `EINVAL`：addr 非页对齐 / length==0 / 溢出 / 部分 unmap
//!   （基址命中但长度不符——v1 无区间拆分）；**未映射区间 → 0**（POSIX
//!   语义：非错误——Linux/musl 实测同形，与 A7-0 草案原文「未命中
//!   EINVAL」的偏差经 A7-1 审查追认取 POSIX 档，guest 钉板）；成功 → 0。
//!   dormant 条目照常收口（unmap 错误忽略——无翻译无可拆；close VMO
//!   无映射无 -EBUSY）。
//! - mprotect（A7-3 dormant 五臂，A7-1「命中恒 EACCES」处置升级）：
//!   addr 非页对齐/len==0 → `EINVAL`；未命中/基址命中但 len 不符 →
//!   `EINVAL`；hv==0（无自 vmar gift）→ `EACCES` 守门；active 档 +
//!   PROT_NONE → **unmap 撤翻译 + dormant=true**（区间不可访问——
//!   读写即 D-abort；VMO 滞表保内容）→ 0；active 档 + 恰 RW（EXEC
//!   忽略）→ no-op 0；dormant 档 + PROT_NONE → 幂等 no-op 0（与
//!   active+RW 对称裁量）；dormant 档 + 恰 RW → **复映复活** +
//!   dormant=false → 0（复映失败 ENOMEM 维持 dormant）；其余组合
//!   （单 READ/单 WRITE/未知位）→ `EACCES`（v1 无只读/降权映射档，
//!   诚实拒）。madvise 维持 stubs.rs ok! 桶（建议性接口，谎报成功 =
//!   「建议被忽略」合法档）。

use core::sync::atomic::AtomicU32;

use berry_fsclient::{get_chunk, stat as rpc_stat, FdEntry, RPC_BUF, TYPE_DIR};
use berry_mmap::{
    classify, round_up_page, MapKind, Region, RegionTable, TakeOutcome, Reject, PAGE,
};
use berry_sys::{
    berry_handle_close, berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create,
    berry_vmo_write, Handle, RIGHT_READ, RIGHT_WRITE,
};

use crate::io::self_vmar_h;
use crate::{c_int, c_void, off_t, set_errno, EBADF, EACCES, EISDIR, EINVAL, EIO, ENOMEM};

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
/// `file` = 文件档参数（fd 路径快照 + 文件内起点 + 拷贝字节数）——
/// 锁外预检已定长（offset+want ≤ size 已证）；匿名档 None。
/// 返 Ok(base) 或 Err(errno)。
unsafe fn mmap_locked(
    hv: u32,
    len: usize,
    file: Option<(FdEntry, u32, usize)>,
) -> Result<usize, c_int> {
    // SAFETY: 见 REGIONS 注。
    let reg = unsafe { &mut *core::ptr::addr_of_mut!(REGIONS) };
    if reg.is_full() {
        return Err(ENOMEM); // 表满前置拒：零 syscall、零内核对象诞生
    }
    let Some(base) = reg.first_fit(len) else {
        return Err(ENOMEM); // 窗内无整洞（first-fit 窗尽）
    };
    // —— vmo → commit → (staging 覆写) → map（失败即回滚，零 handle 泄漏）——
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        return Err(ENOMEM);
    }
    // 整段 commit：内核逐页出页清零（C-2c「逐页 VMO commit」纪律的单
    // syscall 表达；物理不连续无碍——pthread.rs 栈同款先例）。文件档
    // **含 [want, len) 尾零页**——未 commit 页映射读 = D-abort，取整
    // 长全段 commit 后 staging 只需覆写数据前缀（与匿名档同型零新
    // 内核行为假设）。
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        let _ = berry_handle_close(vmo); // 未映射——close 无 -EBUSY 面
        return Err(ENOMEM);
    }
    if let Some((ent, file_off, want)) = file {
        // —— 文件档 staging：get_chunk 直 RPC 循环覆写 [0, want) ——
        // 不经 fs.rs RA 缓冲（fd 游标/RA 状态零扰动）；rpc() 零锁实证
        // （重试 + wait 内核对象往返）——MMAP_LOCK 临界区内无锁序嵌套面。
        let h = crate::io::fs_h();
        let mut buf = [0u8; RPC_BUF];
        let mut done = 0usize; // 已落 VMO 字节数
        let mut src = file_off; // 文件内游标（按 RPC 实读推进）
        while done < want {
            let (n, _) = match get_chunk(h, ent.wire(), src, &mut buf) {
                Ok(r) => r,
                Err(e) => {
                    let _ = berry_handle_close(vmo); // 半途未映——close 无 -EBUSY 面
                    return Err(crate::fs::errno_of(e));
                }
            };
            if n == 0 {
                // EOF 提前到：锁外预检已证 offset+want ≤ size——结构性
                // 不可达的防御回滚（半拷 staging 不上线，宁可 EIO）。
                let _ = berry_handle_close(vmo);
                return Err(EIO);
            }
            let take = core::cmp::min(n, want - done); // 收尾截断（一次即止）
            if berry_vmo_write(vmo, buf[..take].as_ptr(), done as u64, take) != 0 {
                // 刚 commit 的页写必成——结构性不可达的防御回滚。
                let _ = berry_handle_close(vmo);
                return Err(EIO);
            }
            done += take;
            src = src.wrapping_add(n as u32);
        }
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
    if !reg.insert(Region { base, len, vmo: vmo as usize, dormant: false }) {
        // is_full 前置已拦——结构性不可达的防御回滚（零 handle 泄漏）。
        let _ = berry_vmar_unmap(hv, base, len);
        let _ = berry_handle_close(vmo);
        return Err(ENOMEM);
    }
    Ok(base)
}

/// mmap 真身（A7-1 匿名档 / A7-3 文件档 staging；原 stubs.rs ENOMEM 空桩
/// 退役）。
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
    let (len, file) = match classify(length, prot, flags, fd, offset) {
        Ok(MapKind::Anon(l)) => (l, None),
        Ok(MapKind::File(l)) => match mmap_file_precheck(fd, offset, length) {
            Ok(ent) => (l, Some((ent, offset as u32, length))),
            Err(e) => {
                set_errno(e);
                return MAP_FAILED;
            }
        },
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
    let r = unsafe { mmap_locked(hv, len, file) };
    crate::futex::mutex_unlock(&MMAP_LOCK);
    match r {
        Ok(base) => base as *mut c_void,
        Err(e) => {
            set_errno(e);
            MAP_FAILED
        }
    }
}

/// 文件档锁外预检（A7-3）：fd 快照（EBADF）→ 恰 O_RDONLY（EACCES）→
/// STAT 定长（EISDIR/超 EOF EINVAL/RPC 错 errno_of）。返 FdEntry 快照
/// （staging 循环复用其 wire——fd 游标不动）。
fn mmap_file_precheck(fd: c_int, offset: off_t, length: usize) -> Result<FdEntry, c_int> {
    // ① fd 快照：fd<3 console 段/表查无 → EBADF（mmap_fd_probe 口径）。
    let ent = crate::fs::mmap_fd_probe(fd).ok_or(EBADF)?;
    // ② 只收恰 O_RDONLY：O_RDWR/O_WRONLY fd 是共享档的面——v1 无
    //    MAP_SHARED 档，EACCES 诚实拒（POSIX 偏差清单 §3.3）。
    if !ent.readable || ent.writable {
        return Err(EACCES);
    }
    // ③ STAT 定长：目录拒（open 段同口径）；offset+length 超 EOF 用
    //    **原始 length** 判（取整 len 判会把非页倍文件结构性变不可映射）。
    match rpc_stat(crate::io::fs_h(), ent.wire()) {
        Ok((TYPE_DIR, _)) => Err(EISDIR),
        Ok((_, size)) => {
            if offset as u64 + length as u64 > size as u64 {
                Err(EINVAL)
            } else {
                Ok(ent)
            }
        }
        Err(e) => Err(crate::fs::errno_of(e)),
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

/// mprotect dormant 五臂真身（A7-3；A7-1「命中恒 EACCES」处置升级，
/// stubs.rs 零参 EINVAL 桩 A7-1 已退役，签名对真原型 3 参）。头注范围
/// 盒五臂清单的执法点：PROT_NONE = unmap 撤翻译 + dormant（VMO 滞表
/// 保内容）；恰 RW = no-op（active）/ 复映复活（dormant）；其余组合
/// EACCES 诚实拒（v1 无只读/降权映射档）。
#[no_mangle]
pub extern "C" fn mprotect(addr: *mut c_void, len: usize, prot: c_int) -> c_int {
    let a = addr as usize;
    if a % PAGE != 0 || len == 0 {
        set_errno(EINVAL);
        return -1;
    }
    // hv 守门：无自 vmar = 无 unmap/复映能力——EACCES（mmap 守门同型；
    // 且防「unmap 失败但 dormant 翻转」的状态错乱）。
    let hv = self_vmar_h();
    if hv == 0 {
        set_errno(EACCES);
        return -1;
    }
    const PROT_KNOWN: c_int = PROT_READ | PROT_WRITE | PROT_EXEC; // =7
    // 恰 RW 判（EXEC 忽略——classify 同款裁量；未知位/缺位 → false → EACCES 桶）
    let is_none = prot == PROT_NONE;
    let is_rw = prot & !PROT_KNOWN == 0
        && prot & (PROT_READ | PROT_WRITE) == (PROT_READ | PROT_WRITE);
    crate::futex::mutex_lock(&MMAP_LOCK);
    // SAFETY: 见 REGIONS 注（持锁临界区；unmap/map syscall 走 mmap_locked
    // 锁内三 syscall 先例——rpc 零锁、无锁序嵌套面）。
    let r = unsafe {
        let reg = &mut *core::ptr::addr_of_mut!(REGIONS);
        let hit = match reg.find_mut(a) {
            None => None,
            Some(r) if r.len != len => None, // 基址命中长度不符 → EINVAL 桶
            Some(r) => Some(r),
        };
        match hit {
            None => Err(EINVAL), // 未命中 / len 不符（munmap Partial 同口径）
            Some(r) if !r.dormant => {
                // —— active 档 ——
                if is_none {
                    // PROT_NONE：unmap 撤翻译（此后该区间读写 = D-abort——
                    // 调用方纪律）；VMO 滞表保内容。unmap 错误忽略（无
                    // 翻译残余的防御面：失败 = 已无翻译，语义已达标）。
                    let _ = berry_vmar_unmap(hv, r.base, r.len);
                    r.dormant = true;
                    Ok(())
                } else if is_rw {
                    Ok(()) // active+恰 RW：no-op（恒 RW 硬映射，权限同）
                } else {
                    Err(EACCES) // 单 READ/未知位：v1 无降权档
                }
            }
            Some(r) => {
                // —— dormant 档 ——
                if is_none {
                    Ok(()) // 幂等 no-op（与 active+RW 对称裁量）
                } else if is_rw {
                    // 复活：RW 复映（v1 唯一档）；失败 ENOMEM 维持
                    // dormant 零泄漏——VMO 仍滞表，munmap 可收口。
                    let mut va = r.base;
                    if berry_vmar_map(
                        hv,
                        0,
                        r.vmo as Handle,
                        0,
                        &mut va,
                        r.len,
                        RIGHT_READ | RIGHT_WRITE,
                    ) != 0
                        || va != r.base
                    {
                        Err(ENOMEM)
                    } else {
                        r.dormant = false;
                        Ok(())
                    }
                } else {
                    Err(EACCES)
                }
            }
        }
    };
    crate::futex::mutex_unlock(&MMAP_LOCK);
    match r {
        Ok(()) => 0,
        Err(e) => {
            set_errno(e);
            -1
        }
    }
}
