//! pthread 最小真身（T-1 §3）—— std::thread 的全消费面
//!
//! 形态（草案 §3 + 主会话三裁决）：
//! - **struct pthread** 嵌线程块 TLS 窗尾：子线程块 = malloc(洞 0x10 +
//!   TLS 窗 + struct)，`tls_place` 同一算术放窗（TP = data−0x10），洞
//!   `[TP, TP+0x10)` 写 struct 自指针（`pthread_self` 读点）；**裁决二：
//!   无 join_ev**——join 走内核 Thread TERMINATED 信号面（`object_wait_one`
//!   在「任务永不再跑点」后返——清栈零竞速）。
//! - **栈**：vmo_create+commit+vmar_map（gift[3] 自描述 Vmar——T-1 前缀紧致布局固定位），VA = 静态
//!   原子 bump 从 0x6000_0000 起（区间 256MiB，与映像/主栈/MMIO
//!   0x7000_0000/USER_TOP 全不撞）；栈底保护页不映射（穿栈 = 干净
//!   D-abort，hexyl 教训）。无复用（~127 个 2MiB 生命周期，v1 文档化）。
//! - **蹦床** `thread_entry`：装 TPIDR_EL0 **先于一切 TLS 访问**（此前只经
//!   裸指针/纯函数）→ TLS 窗显式清零+拷模板 → 洞写自指针 → start_routine
//!   → retval → run_dtors（4 轮）→ state=Exited → free(Parcel) →
//!   `berry_thread_exit`（仅退本线程；TERMINATED 由内核退出钩子置位）。
//! - **key 族**：PKEY_DTOR[64] 进程级（futex 锁保键分配）+ specific[]
//!   per-thread；pthread_key_create 必须成功（std rtabort = abort 级——
//!   64 槽对 std 数枚键结构性无限）。
//! - **sem 三件**（ctrlc 后台线程阻塞面——必须与 pthread 同刀）：计数 ≤0
//!   → futex 真睡（v1 无信号面 sem_post 永不触发 = 永睡无害；EAGAIN 桩
//!   的热自旋是其反面）。
//! - detached 线程已退出且无人 join → 资源随进程死 sweep（有界文档化）。
//! - **守门**：self_proc_h()/self_vmar_h()==0（老 spawn 面，无自描述对
//!   gift）或 MEMSZ==0（无 PT_TLS，bin/hello 类）→ EAGAIN——旧桩语义
//!   零漂移，std Builder::spawn 走 Err 分支。

use core::arch::asm;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};

use berry_sys::{
    berry_handle_close, berry_object_wait_one, berry_thread_create, berry_thread_exit,
    berry_thread_start, berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create,
    Handle, BERRY_ERR_TIMEDOUT, RIGHT_READ, RIGHT_WRITE, SIGNAL_TERMINATED,
};

use crate::crt0::tls_place;
use crate::futex::mutex_lock;
use crate::{c_int, c_void, size_t};

/// musl aarch64：`pthread_t` = 整数柄（struct pthread *）。
pub type pthread_t = usize;

const PAGE: usize = 4096;
/// 默认栈 = 2MiB（std 默认档；attr 可覆盖）。
const DEFAULT_STACK: usize = 2 << 20;
/// 子线程栈 VA bump 区基（映像 0x1000+ / 主栈 / MMIO 0x7000_0000 /
/// USER_TOP 1<<39 全不撞）。
const STACK_VA_BASE: usize = 0x6000_0000;
/// bump 区间 256MiB（无复用：~127 个 2MiB 生命周期；du/sort 3 线程远够）。
const STACK_VA_SPAN: usize = 256 << 20;
/// 内核 `VMO_MAX_SIZE` 同值（C-2c 起 32MiB）——栈超此 vmo_create 必拒。
const VMO_MAX: usize = 32 << 20;
const PKEYS: usize = 64;

/// 线程控制块（#[repr(C)]；嵌子线程块 TLS 窗尾——`specific` 之后的字段
/// 顺序无 ABI 消费者，布局是本 libc 私有）。
#[repr(C)]
pub struct pthread {
    pub th_h: Handle,
    /// start_routine 返回值（join 读）。
    pub retval: usize,
    /// 0=Live 1=Exited 2=Joined。
    pub state: u8,
    pub detached: bool,
    pub tls_base: *mut u8,
    pub tls_total: usize,
    pub stack_vmo: Handle,
    /// 栈映射基（保护页之上）；unmap 须精确匹配此区间。
    pub stack_base: usize,
    pub stack_len: usize,
    /// per-thread TLS 键值（键从 1 起 → 槽 k-1）。
    pub specific: [usize; PKEYS],
}

/// 主线程块（无堆块：arena 洞保持清零 → `current_pd` 回落至此；不可 join）。
static mut MAIN_PTHREAD: pthread = pthread {
    th_h: 0,
    retval: 0,
    state: 0,
    detached: false,
    tls_base: core::ptr::null_mut(),
    tls_total: 0,
    stack_vmo: 0,
    stack_base: 0,
    stack_len: 0,
    specific: [0; PKEYS],
};

/// 蹦床参数包（malloc 块；线程入口读后在线程退出前 free）。
struct Parcel {
    start: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
    pd: *mut pthread,
}

static STACK_NEXT: AtomicUsize = AtomicUsize::new(STACK_VA_BASE);

// —— key 登记（进程级；futex 锁保分配）——

static mut PKEY_DTOR: [Option<extern "C" fn(*mut c_void)>; PKEYS] = [None; PKEYS];
static mut PKEY_N: usize = 0;
static PKEY_LOCK: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn pthread_key_create(
    key: *mut u32,
    dtor: extern "C" fn(*mut c_void),
) -> c_int {
    if key.is_null() {
        return crate::EINVAL;
    }
    unsafe {
        mutex_lock(&PKEY_LOCK);
        if PKEY_N >= PKEYS {
            crate::futex::mutex_unlock(&PKEY_LOCK);
            return crate::EAGAIN; // 不可达档（std 数枚键 vs 64 槽）
        }
        PKEY_DTOR[PKEY_N] = Some(dtor);
        PKEY_N += 1;
        *key = PKEY_N as u32; // 从 1 起（旧桩同口径零漂移）
        crate::futex::mutex_unlock(&PKEY_LOCK);
    }
    0
}

#[no_mangle]
pub extern "C" fn pthread_key_delete(_key: u32) -> c_int {
    0 // 登记不回收（v1 计数器无槽复用；泄漏面 = dtor 槽有界）
}

#[no_mangle]
pub extern "C" fn pthread_setspecific(key: u32, val: *const c_void) -> c_int {
    if key == 0 || key as usize > PKEYS {
        return crate::EINVAL;
    }
    unsafe {
        (*current_pd()).specific[(key - 1) as usize] = val as usize;
    }
    0
}

/// 弱面顺手件（附录 A weak 24 有真消费者；per-thread 化）。
#[no_mangle]
pub extern "C" fn pthread_getspecific(key: u32) -> *mut c_void {
    if key == 0 || key as usize > PKEYS {
        return core::ptr::null_mut();
    }
    unsafe { (*current_pd()).specific[(key - 1) as usize] as *mut c_void }
}

/// 本线程控制块：子线程 = 洞自指针；主线程（arena 洞清零）→ MAIN_PTHREAD。
fn current_pd() -> *mut pthread {
    let tp: usize;
    unsafe { asm!("mrs {}, tpidr_el0", out(reg) tp) };
    if tp != 0 {
        let hole = unsafe { core::ptr::read_volatile(tp as *const usize) };
        if hole != 0 {
            return hole as *mut pthread;
        }
    }
    core::ptr::addr_of_mut!(MAIN_PTHREAD)
}

/// 线程退出前的 key 析构（4 轮——std guard::enable 两轮 DEFER 协议依赖
/// 多轮；一轮内新注销的值下轮再跑）。
unsafe fn run_dtors(pd: *mut pthread) {
    unsafe {
        for _ in 0..4 {
            let mut ran = false;
            for k in 0..PKEYS {
                let v = (*pd).specific[k];
                if v != 0 {
                    (*pd).specific[k] = 0;
                    if let Some(dtor) = PKEY_DTOR[k] {
                        dtor(v as *mut c_void);
                        ran = true;
                    }
                }
            }
            if !ran {
                break;
            }
        }
    }
}

// —— attr 族（musl b64 形状 56B 不透明块；布局私有）——

#[repr(C)]
pub struct pthread_attr_t {
    /// 0 = 默认（2MiB）。
    pub stacksize: usize,
    pub _pad: [u8; 48],
}

#[no_mangle]
pub extern "C" fn pthread_attr_init(attr: *mut pthread_attr_t) -> c_int {
    if attr.is_null() {
        return crate::EINVAL;
    }
    unsafe { core::ptr::write_bytes(attr as *mut u8, 0, 56) };
    0
}

#[no_mangle]
pub extern "C" fn pthread_attr_destroy(_attr: *mut pthread_attr_t) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn pthread_attr_setstacksize(attr: *mut pthread_attr_t, s: size_t) -> c_int {
    if attr.is_null() || s < PAGE {
        return crate::EINVAL;
    }
    unsafe { (*attr).stacksize = s };
    0
}

// —— 身份/生命周期 ——

#[no_mangle]
pub extern "C" fn pthread_self() -> pthread_t {
    current_pd() as pthread_t
}

#[no_mangle]
pub extern "C" fn pthread_create(
    thread: *mut pthread_t,
    attr: *const pthread_attr_t,
    start: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> c_int {
    if thread.is_null() {
        return crate::EINVAL;
    }
    // 守门（老 spawn 面 / 无 PT_TLS 程序 → EAGAIN，旧桩语义零漂移）。
    let hp = crate::io::self_proc_h();
    let hv = crate::io::self_vmar_h();
    let (_, filesz, memsz, align) = crate::crt0::tls_template();
    if hp == 0 || hv == 0 || memsz == 0 {
        return crate::EAGAIN;
    }
    let want = if attr.is_null() {
        0
    } else {
        unsafe { (*attr).stacksize }
    };
    let stack_len = match want {
        0 => DEFAULT_STACK,
        n if n < PAGE => return crate::EINVAL,
        n => (n + PAGE - 1) & !(PAGE - 1),
    };
    if stack_len > VMO_MAX {
        return crate::EAGAIN; // vmo_create 上限外——诚实拒
    }
    unsafe {
        // 线程块：洞 0x10 + 对齐余量 + TLS 窗（memsz 取整 8）+ struct pthread。
        let align8_memsz = (memsz + 7) & !7;
        let total = (0x10 + align + align8_memsz + core::mem::size_of::<pthread>() + 15) & !15;
        let base = crate::alloc::malloc(total) as *mut u8;
        if base.is_null() {
            return crate::EAGAIN;
        }
        crate::strmem::memset_volatile(base, 0, total); // 具体字段前整块清零
        let Some(data) = tls_place(base as usize, total, filesz, memsz, align) else {
            crate::alloc::free(base as *mut c_void);
            return crate::EAGAIN; // total 公式已覆盖——结构性不可达的防御面
        };
        let pd = (data + align8_memsz) as *mut pthread;
        // 栈：保护页 [va, va+PAGE) 不映射；映射区 = [va+PAGE, +stack_len)。
        let va = STACK_NEXT.fetch_add(stack_len + PAGE, Ordering::Relaxed);
        let rollback_block = |extra: Option<*mut c_void>| {
            if let Some(p) = extra {
                crate::alloc::free(p);
            }
            crate::alloc::free(base as *mut c_void);
        };
        if va + stack_len + PAGE > STACK_VA_BASE + STACK_VA_SPAN {
            rollback_block(None);
            return crate::EAGAIN; // bump 区尽（v1 无复用，文档化）
        }
        let mut svmo = 0 as Handle;
        if berry_vmo_create(stack_len as u64, 0, &mut svmo) != 0 || svmo == 0 {
            rollback_block(None);
            return crate::EAGAIN;
        }
        if berry_vmo_commit(svmo, 0, stack_len as u64) != 0 {
            let _ = berry_handle_close(svmo);
            rollback_block(None);
            return crate::EAGAIN;
        }
        let map_base = va + PAGE;
        let mut saddr = map_base;
        if berry_vmar_map(hv, 0, svmo, 0, &mut saddr, stack_len, RIGHT_READ | RIGHT_WRITE) != 0
            || saddr != map_base
        {
            let _ = berry_handle_close(svmo);
            rollback_block(None);
            return crate::EAGAIN;
        }
        let unmap_stack = |sv: Handle| {
            let _ = berry_vmar_unmap(hv, map_base, stack_len);
            let _ = berry_handle_close(sv);
        };
        let mut th = 0 as Handle;
        if berry_thread_create(hp, 0, &mut th) != 0 || th == 0 {
            unmap_stack(svmo);
            rollback_block(None);
            return crate::EAGAIN;
        }
        let parcel = crate::alloc::malloc(core::mem::size_of::<Parcel>()) as *mut Parcel;
        if parcel.is_null() {
            let _ = berry_handle_close(th);
            unmap_stack(svmo);
            rollback_block(None);
            return crate::EAGAIN;
        }
        // struct 字段全部就位**先于** thread_start（新线程一跑即读它；
        // specific[] 已随整块清零归零）。
        (*pd).th_h = th;
        (*pd).retval = 0;
        (*pd).state = 0; // Live
        (*pd).detached = false;
        (*pd).tls_base = base;
        (*pd).tls_total = total;
        (*pd).stack_vmo = svmo;
        (*pd).stack_base = map_base;
        (*pd).stack_len = stack_len;
        (*parcel) = Parcel { start, arg, pd };
        let sp = map_base + stack_len; // 栈顶（页倍数 → 16 对齐）
        if berry_thread_start(th, thread_entry as *const () as usize, sp, parcel as usize) != 0 {
            let _ = berry_handle_close(th);
            unmap_stack(svmo);
            rollback_block(Some(parcel as *mut c_void));
            return crate::EAGAIN;
        }
        *thread = pd as pthread_t;
        0
    }
}

/// 蹦床：新线程入口（x0 = Parcel 指针；sp = 栈顶）。`#[inline(never)]`
/// 保独立帧。**msr 前零 TLS 访问**（只经裸指针/纯函数——本函数体内不触
/// 任何 `#[thread_local]`）。
#[inline(never)]
unsafe extern "C" fn thread_entry(p: *mut Parcel) -> ! {
    unsafe {
        let parcel: &Parcel = &*p;
        let pd: *mut pthread = parcel.pd;
        let (tva, filesz, memsz, align) = crate::crt0::tls_template();
        let data = match tls_place((*pd).tls_base as usize, (*pd).tls_total, filesz, memsz, align)
        {
            Some(d) => d,
            // create 侧同参已验 Some——结构性不可达，brk loud（crt0 #0x7C1
            // 同款纪律）。
            None => {
                asm!("brk #0x7C2");
                loop {
                    core::hint::spin_loop();
                }
            }
        };
        let tp = (data - 0x10) as u64;
        asm!("msr tpidr_el0, {}", in(reg) tp); // EL0 可写（P6 A-3 事实）
        // TLS 窗装配：显式清零整窗 + 拷模板（法证教训一级防御；窗在
        // struct pthread 之下互不覆写——create 侧已清零，此处双保险）。
        crate::strmem::memset_volatile(data as *mut u8, 0, memsz);
        crate::strmem::memcpy_volatile(data as *mut u8, tva as *const u8, filesz);
        // 洞 [TP, TP+0x10) 写 struct pthread 自指针（pthread_self 读点）。
        core::ptr::write_volatile(tp as *mut usize, pd as usize);
        // 用户主体。
        let ret = (parcel.start)(parcel.arg);
        (*pd).retval = ret as usize;
        run_dtors(pd);
        (*pd).state = 1; // Exited（TERMINATED 由内核退出钩子置位——裁决二）
        crate::alloc::free(p as *mut c_void); // Parcel 使命已毕
        berry_thread_exit(); // 仅退本线程（非末线程不收口进程）
    }
}

/// join = 等内核 Thread TERMINATED（裁决二：任务永不再跑点置位——清栈
/// 零竞速；用户态 Event join 的「joiner 拔垂死线程栈」竞速实锤已弃）。
/// 双 join 未定义（首 join 已 free 控制块——musl 同档不设防）。
#[no_mangle]
pub extern "C" fn pthread_join(t: pthread_t, value_ptr: *mut *mut c_void) -> c_int {
    let pd = t as *mut pthread;
    let main_pd = core::ptr::addr_of_mut!(MAIN_PTHREAD);
    if pd.is_null() || pd == main_pd {
        return crate::EINVAL; // 主线程不可 join
    }
    unsafe {
        if (*pd).detached {
            return crate::EINVAL;
        }
        let mut obs = 0u64;
        let s = berry_object_wait_one((*pd).th_h, SIGNAL_TERMINATED, u64::MAX, &mut obs);
        if s != 0 && s != BERRY_ERR_TIMEDOUT {
            return crate::ESRCH; // handle 失效/类型错（u64::MAX 实际不到点）
        }
        if !value_ptr.is_null() {
            *value_ptr = (*pd).retval as *mut c_void;
        }
        let hv = crate::io::self_vmar_h();
        if hv != 0 {
            let _ = berry_vmar_unmap(hv, (*pd).stack_base, (*pd).stack_len);
        }
        let _ = berry_handle_close((*pd).stack_vmo);
        let _ = berry_handle_close((*pd).th_h);
        (*pd).state = 2; // Joined（写后即 free——簿记纯粹性）
        crate::alloc::free((*pd).tls_base as *mut c_void);
        // A7-4 ③b：join 等醒补投（§3.3——清理完毕再投，handler 面见到的
        // 是已回收的线程簿记）。仅成功路径：错码路径无等待窗。
        crate::signal::may_dispatch();
        0
    }
}

/// 标位；已 Exited 且无人 join → 资源随进程死 sweep（有界文档化）。
#[no_mangle]
pub extern "C" fn pthread_detach(t: pthread_t) -> c_int {
    let pd = t as *mut pthread;
    if pd.is_null() {
        return crate::EINVAL;
    }
    unsafe {
        (*pd).detached = true;
    }
    0
}

// —— sem 三件真身（ctrlc 后台线程阻塞面——必须与 pthread 同刀）——
// musl b64 sem_t = 32B、计数位偏移 0。计数位只经原子面（wait 减/post 加，
// 不出负——ctrlc 的 sem_init(0) 形态下 futex expected=0 恒成立）。

#[no_mangle]
pub extern "C" fn sem_init(sem: *mut c_void, _pshared: c_int, value: c_int) -> c_int {
    if sem.is_null() {
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    unsafe {
        core::ptr::write_bytes(sem as *mut u8, 0, 32);
        core::ptr::write_volatile(sem as *mut i32, value);
    }
    0
}

#[no_mangle]
pub extern "C" fn sem_wait(sem: *mut c_void) -> c_int {
    if sem.is_null() {
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    let c = sem as *mut AtomicI32;
    unsafe {
        loop {
            let v = (*c).load(Ordering::Acquire);
            if v > 0 {
                if (*c)
                    .compare_exchange(v, v - 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    return 0;
                }
                continue; // 竞争失败重读
            }
            // v == 0：futex 真睡（v1 无信号面永不触发 = 永睡无害——烧片
            // 自旋是 EAGAIN 桩的反面教训）；伪醒/防御超时都重验。
            let r = crate::futex::futex_wait(c as usize, 0, u64::MAX);
            if r == crate::ETIMEDOUT {
                continue;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn sem_post(sem: *mut c_void) -> c_int {
    if sem.is_null() {
        crate::set_errno(crate::EINVAL);
        return -1;
    }
    let c = sem as *mut AtomicI32;
    unsafe {
        (*c).fetch_add(1, Ordering::Release);
    }
    let _ = crate::futex::futex_wake(c as usize);
    0
}
