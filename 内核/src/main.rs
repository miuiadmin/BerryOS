//! bos-kernel —— BerryOS 内核镜像。
//!
//! Phase 2a Step 6（里程碑）：两内核任务在 CNTV 定时器上**抢占式分时**。`on_irq_tick`
//! 经 IRQ 派发槽被调，做真 `tick`（减 slice_left）+ 归 0 则 `pick_next` + `switch`。
//! task_a/task_b 是**忙循环**（不 yield）——A/B 交替**纯由 timer slice 驱动**，即里程碑
//! 原文「两个内核任务在定时器上分时」。每 50 次切换打一行 `switches=N` 金丝雀（liveness）。
//! 异常向量已装（HAL `trap`）；panic handler 输出消息 + 位置（file:line:col）（G0 判据），
//! CPU 异常寄存器/ESR/FAR dump 在 HAL `trap::exception_sync`。
#![no_std]
#![no_main]

extern crate alloc;

mod aspace;
mod channel;
mod handle;
mod heap;
mod ktest;
mod mm;
mod process;
mod syscall;
mod user;
mod vmar;
mod vmo;

use heap::SlabHeap;

/// 全局分配器：内核 slab 堆（小对象走 slab，大对象走整页）。
#[global_allocator]
static ALLOC: SlabHeap = SlabHeap::new();

/// 把 `core::fmt` 格式串直接打到 UART（无缓冲、逐字节）——panic 消息/位置用。
struct UartWriter;

impl core::fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        bos_hal_aarch64::uart_puts(s.as_bytes());
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // G0 判据：panic 必须有人可读的 dump。消息 + file:line:col。
    // （CPU 异常的寄存器/PC/ESR/FAR dump 另走 HAL `trap::exception_sync`。）
    let _ = writeln!(UartWriter, "\nKERNEL PANIC");
    if let Some(loc) = info.location() {
        let _ = writeln!(UartWriter, "  at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    // PanicMessage 实现了 Display（1.95 起 message() 不再返回 Option）。
    let _ = writeln!(UartWriter, "  {}", info.message());
    bos_hal_aarch64::halt();
}

/// 内核 Rust 入口（由 HAL boot asm 的 `br rust_main` 调用——2b-1 起高半入口）。
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配。
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // 2b-1：注入 direct map 基址——必须先于任何 phys_to_virt 消费者（mm/heap）。
    bos_mm::va::set_kernel_va_base(bos_hal_aarch64::KERNEL_VA_BASE);
    bos_hal_aarch64::init();
    bos_hal_aarch64::uart_puts(b"BerryOS boot\n");

    // 2b-0：TTBR1 高半表自校验（RAM 魔数 + GIC PIDR2 双别名，见 HAL `mmu`）。
    // 不过 = 高半翻译未生效，2b-1 重链前必须在此暴露——panic 停机优于带病前进。
    if !bos_hal_aarch64::kva_ok() {
        panic!("KVA alias check failed: TTBR1 high-half not translating");
    }
    bos_hal_aarch64::uart_puts(b"KVA ok\n");

    // 2b-2：低半身份表退役——TTBR0 ← 内核空表 + `tlbi vmalle1is`。此后低半任何
    // 访问必然 fault（ktest ASPACE 用例正向断言）；UART/GIC MMIO 已走高半直映，
    // 内核对低半零依赖。回退开关见 HAL `mmu::RETIRE_IDENTITY`。
    bos_hal_aarch64::retire_low_half();

    // SAFETY: 启动单线程、`rust_main` 入口仅调用一次。
    unsafe { mm::init() };

    // 3-1：注册真 syscall 语义 handler（handle 三操作 + clock_read）进 HAL 表。
    // 必须先于 ktest::run（SYSCALL 用例）与一切用户进程。
    syscall::install();

    ktest::run();

    // Phase 2a Step 5：IRQ 派发钩子 + timer 管线（规律 tick，**不从 IRQ 调 switch**）。
    // SAFETY: EL1、启动单线程、仅一次：缓存 CNTFRQ/算 SLICE（不武装）。
    unsafe { bos_hal_aarch64::timer::init() };

    // 2b-2：两忙循环任务各挂一个私有地址空间——timer 抢占切任务 = 切 TTBR0/ASID
    // （switch 路径真 churn），任务体每次打印节流点自证私有页内容（隔离金丝雀）。
    // `Box::leak`：AS 须与任务同寿（v1 任务不退出）。
    let as_a: &'static aspace::MiniAspace = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        aspace::MiniAspace::new(mm::frames(), aspace::alloc_asid(), TASK_A_MAGIC),
    ));
    let as_b: &'static aspace::MiniAspace = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        aspace::MiniAspace::new(mm::frames(), aspace::alloc_asid(), TASK_B_MAGIC),
    ));

    // IRQ 此刻仍屏蔽（boot DAIF）；spawn 要求屏蔽态（alloc/enqueue 不可打断）。
    bos_sched::init();
    bos_sched::spawn_as(task_a, TASK_A_MAGIC as usize, b"task_a", as_a.ttbr0_pa, as_a.asid);
    bos_sched::spawn_as(task_b, TASK_B_MAGIC as usize, b"task_b", as_b.ttbr0_pa, as_b.asid);

    // 2b-4（§5.1 第 5 步收官）：首批 EL0 用户进程 + G1.5 隔离关卡。good 打 'u'
    // （syscall 往返在 EL0 生效的活证）；rogue_ptr 野指针自作死 → 只死自己
    // （user_fault_exit）；abuser 坏 handle 滥用 → **-EBADF 存活**（3-1 撤
    // 「畸形号杀」政策：syscall 层错误一律错误码自理）。watchdog 断言后打关卡标记。
    let mut up_good =
        user::UserProcess::build(mm::frames(), user::user_good_code(), user::USER_MAGIC);
    let up_rogue_ptr =
        user::UserProcess::build(mm::frames(), user::user_rogue_ptr_code(), user::USER_MAGIC);
    let up_abuser =
        user::UserProcess::build(mm::frames(), user::user_abuser_code(), user::USER_MAGIC);
    let up_spawner =
        user::UserProcess::build(mm::frames(), user::user_spawner_code(), user::USER_MAGIC);

    // 3-4a：per-process 表拆分——每个 boot 手搭用户进程一张 leak 常驻表
    //（任务 tag 指向它，EL0 syscall 派发到自己的表；v1 无进程退出，表同寿）。
    use handle::{HandleTable, KernelObj, RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP};
    let ht_good: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_rogue: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_abuser: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_spawner: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let (tag_good, tag_rogue, tag_abuser, tag_spawner) = (
        ht_good as *mut HandleTable as usize,
        ht_rogue as *mut HandleTable as usize,
        ht_abuser as *mut HandleTable as usize,
        ht_spawner as *mut HandleTable as usize,
    );

    // 3-2：good 的 root Vmar 进**它自己的表** = 首能力（能力模型 §5「第一个
    // 能力从哪来」——用户拿自己的地址空间根，MAP|MANAGE|DUPLICATE）。handle
    // 作 spawn arg → 入口 x0 → good 的 3-2 验收链经真 syscall map 读写自己的
    // VMO（全链落在 good 自己的表里——per-process 拆分后其余进程看不见）。
    let good_vmar = up_good.vmar.take().expect("main: good vmar");
    let good_vmar_h = ht_good
        .insert(KernelObj::Vmar(good_vmar), RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE)
        .expect("main: 插 root Vmar 进 good 表");

    bos_sched::spawn_user(user::USER_CODE_VA, good_vmar_h as usize, user::USER_STACK_TOP,
                          b"user_good", up_good.ttbr0_pa, up_good.asid, tag_good);
    bos_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_rogue_ptr", up_rogue_ptr.ttbr0_pa, up_rogue_ptr.asid, tag_rogue);
    bos_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_abuser", up_abuser.ttbr0_pa, up_abuser.asid, tag_abuser);
    // 3-3/3-4 验收：spawner 经真 syscall 创建第二个进程；process_start 的
    // bootstrap 迁移把「礼物 VMO」handle 送进子进程表（子入口 x0 收到后读出
    // 魔数自证——跨进程能力流动，见 user.rs spawner 模板注释）。
    bos_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_spawner", up_spawner.ttbr0_pa, up_spawner.asid, tag_spawner);
    bos_sched::spawn(user::watchdog, 0, b"watchdog");

    // 注册回调（HAL→sched 反转，避循环依赖；均须在 enable_irq / 首个用户任务被
    // 调度之前）。IRQ 槽驱动抢占；用户故障槽驱动 G1.5 kill。
    bos_hal_aarch64::set_irq_dispatch(bos_sched::on_irq_tick);
    bos_hal_aarch64::set_user_fault_exit(bos_sched::user_fault_exit);
    // 武装首个时间片 + 解蔽 IRQ：之后 timer 每 slice（10Hz）落 exception_irq → on_irq_tick。
    // SAFETY: 写 CNTV_CTL/CVAL（绝对截止）；EL1。arm_slice 设 ENABLE+不屏蔽（0b01）。
    unsafe { bos_hal_aarch64::timer::arm_slice() };
    bos_hal_aarch64::enable_irq();
    bos_sched::yield_now(); // idle(main) 让出 → task_a；timer 每 slice 抢占切到 task_b → A/B 交替。
    // 不可达（忙循环任务不退出）；防御兜底：若被切回则进 tickless idle。
    bos_sched::idle_loop();
}

/// 忙循环任务 A（Step 5/6 demo）：纯忙转、不打 yield——证 timer 驱动。
/// Step 5：`on_irq` 不 switch → task_a 独占、task_b 饿死（无 `B` 输出）。
/// Step 6：`on_irq` 接 switch → task_a/task_b 在 slice 下交替。
/// 2b-2：arg = 本任务私有页 magic；每次打印节流点经当前 TTBR0 读私有页自证
/// （隔离金丝雀——切错表/泄漏立即 panic 暴露）。
extern "C" fn task_a(arg: usize) -> ! {
    let magic = arg as u64;
    static A_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = A_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= A_PRINT_EVERY {
            A_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: 读自身 AS 的私有页（Normal 映射）；错值非安全问题是正确性问题。
            let v = unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) };
            assert_eq!(v, magic, "task_a AS 隔离破坏：私有页读到对端内容");
            bos_hal_aarch64::uart_puts(b"A");
        }
    }
}

/// 忙循环任务 B（同 A，打 `B`）。Step 5 饿死（无切换）；Step 6 被调起。
extern "C" fn task_b(arg: usize) -> ! {
    let magic = arg as u64;
    static B_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = B_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= B_PRINT_EVERY {
            B_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: 同 task_a。
            let v = unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) };
            assert_eq!(v, magic, "task_b AS 隔离破坏：私有页读到对端内容");
            bos_hal_aarch64::uart_puts(b"B");
        }
    }
}

/// A/B 的打印节流阈值（迭代次数）。小→刷屏（直观见交替）；大→低噪（soak 时虚拟时间
/// 不被 UART 背压吃掉，switches 计数贴近 wall-clock × SLICE_HZ）。SOAK=1 为低噪 soak。
const A_PRINT_EVERY: u32 = 2_000_000;
const B_PRINT_EVERY: u32 = 2_000_000;
// good 的节流阈值 3-2 起 baked 进用户模板（movz x9, #0x30, lsl #16 ≈ 3.1M）——
// 入口 x0 改传 root Vmar handle（首能力）。

/// task_a/task_b 私有页 magic（2b-2 隔离金丝雀；互异且非零）。
const TASK_A_MAGIC: u64 = 0x00A5_C0DE_AAAA_0001;
const TASK_B_MAGIC: u64 = 0x00B6_C0DE_BBBB_0002;
