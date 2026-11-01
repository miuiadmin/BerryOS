//! T-1 §2（多线程首刀消费面）：进程内双线程自验子——hello 同款裸 bin，
//! **零 TLS 访问、零 console 依赖、x0 直达 _start 无 crt0**。
//!
//! bootstrap 自句柄形态：svcmgr 侧 `process_start(..., bootstrap = dup(hp,
//! MANAGE|DUPLICATE))` 把本进程 Process 句柄迁入子表、经 x0 递进来——
//! 线程自管面（thread_create 门控 Process MANAGE）由此成立。
//!
//! 五步链（T-1 实施设计草案 §2）：
//!  ⓪ thread_create(self) 建次线程 t2（未 start = 不调度，纯对象）；
//!  ① thread_start(t2, thr2_entry, STACK2 顶, 0)——entry/sp 全程序内地址
//!     （栈是 .bss 内嵌 2048B 对齐 16 数组，非像外新页）；
//!  ② t2 体：写共享哨兵 SENTINEL = MAGIC → thread_exit()（**非末线程
//!     退出不收口进程**——T-1 核心语义，t1 必须还活着才能继续走 ③④⑤）；
//!  ③ t1 wait_one(t2, TERMINATED, now+10s) + observed==TERMINATED +
//!     哨兵==MAGIC（join 面：码错/超时/哨兵缺 → exit 9）；
//!  ④ 复启已 TERMINATED 线程必须 -EBADSTATE（-37）——错码/0 都 exit 9；
//!  ⑤ process_exit(7)——t1 是末线程，此刻才收口进程（码 7 传播给父侧）。
//! 任何一步失败 exit 9；父 svcmgr 按 `7` 打 `THR ok`、否则
//!  `eco: thr exit <n>` 降级行（标记缺席 = smoke FAIL 执法）。
#![no_std]
#![no_main]

use berry_sys::{
    berry_clock_read, berry_object_wait_one, berry_process_exit, berry_thread_create,
    berry_thread_exit, berry_thread_start, Handle, BERRY_ERR_BAD_STATE, CLOCK_MONOTONIC,
    SIGNAL_TERMINATED,
};

/// join 帽：TCG 下次线程 5 指令即退，10s 是宽帽不是竞速（状态谓词帽）。
const JOIN_CAP_NS: u64 = 10 * 1_000_000_000;

/// 次线程栈尺寸：thr2_entry 全链 = 一次 volatile 写 + 一个 svc（thread_exit
/// 不返）——帧面百字节级，2048B ≈ 20x 余量。**不取 8192**：svcmgr 内嵌
/// thrchild 平像 + 自身已 57344B，8192 栈把平像推到 3 页（12288B）→ svcmgr
/// 69632B 撞破 64KiB 帽（kernel `USER_CODE_MAX`——D3 守卫非本刀可动）；
/// 2048 栈 + 哨兵恰共页（mem 0x9a8 < 页），平像 1 页、svcmgr 61440B 留
/// 4KiB 头寸。
const STACK2_SZ: usize = 2048;

/// 次线程栈：.bss 内嵌、16 对齐（内核 thread_start 的 sp%16==0 门）
/// ——`repr(align)` 只收类型不收 static，故包一层单字段 struct（repr(C)
/// 首字段零偏移 = 数组即结构体起点）。elf2flat 平像含 .bss 补零——
/// 栈在镜像内，无像外分配。
#[repr(C, align(16))]
struct Stack2([u8; STACK2_SZ]);
static mut STACK2: Stack2 = Stack2([0; STACK2_SZ]);

/// t1/t2 共享哨兵：t2 写 MAGIC、t1 join 后读验——写读序由 TERMINATED
/// 信号定序（wait 返回 ⟹ 写已发生）。
static mut SENTINEL: u64 = 0;
const SENTINEL_MAGIC: u64 = 0x5448_5232_4d41_47; // "THR2MAG"

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(self_proc: Handle) -> ! {
    unsafe {
        // ⓪ 建次线程 t2（挂 self_proc 表；MANAGE 权由 bootstrap dup 授入）。
        let mut t2: Handle = 0;
        if berry_thread_create(self_proc, 0, &mut t2) != 0 || t2 == 0 {
            berry_process_exit(9);
        }
        // ① 投入运行：entry/arg/sp 四参（T-1 扩形）。栈顶 = 结构体起点 +
        // STACK2_SZ（16 对齐保持）。
        let sp2 = core::ptr::addr_of_mut!(STACK2) as usize + STACK2_SZ;
        if berry_thread_start(t2, thr2_entry as *const () as usize, sp2, 0) != 0 {
            berry_process_exit(9);
        }
        // ③ join：wait_one 绝对 deadline 需先读钟。
        let mut t0: u64 = 0;
        if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
            berry_process_exit(9);
        }
        let mut obs: u64 = 0;
        let joined = berry_object_wait_one(t2, SIGNAL_TERMINATED, t0 + JOIN_CAP_NS, &mut obs);
        if joined != 0
            || obs != SIGNAL_TERMINATED
            || core::ptr::read_volatile(core::ptr::addr_of!(SENTINEL)) != SENTINEL_MAGIC
        {
            berry_process_exit(9);
        }
        // ④ 已 TERMINATED 线程复启必须 -EBADSTATE。
        if berry_thread_start(t2, thr2_entry as *const () as usize, sp2, 0) != BERRY_ERR_BAD_STATE {
            berry_process_exit(9);
        }
        // ⑤ t1 末线程收口，码 7 上抛父侧。
        berry_process_exit(7);
    }
}

/// ② 次线程体：写哨兵即退——thread_exit 不返回（`-> !`）。
#[no_mangle]
fn thr2_entry() -> ! {
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(SENTINEL), SENTINEL_MAGIC);
    }
    berry_thread_exit();
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // 裸 bin 无诊断面：错码 9 = 通用失败（与各步失败同码，父侧降级行承载）。
    berry_process_exit(9);
}
