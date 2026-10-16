//! berry-kernel —— BerryOS 内核镜像。
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
mod dtb;
mod event;
mod handle;
mod heap;
mod ktest;
mod mm;
mod pi;
mod process;
mod syscall;
mod telemetry;
mod user;
mod vmar;
mod vmo;

use berry_mm::va::phys_to_virt;
use heap::SlabHeap;

/// G-eco 入机侧：boot 期装载的 rootfs 原字节（fw_cfg / DTB 双源合一产物）。
/// 消费点随下一步「包只读 VMO 交 svcmgr bootstrap」；boot 单线程写、之后
/// 只读，不进并发路径。
static mut ROOTFS: Option<alloc::vec::Vec<u8>> = None;

/// rootfs 大小上限——与 `vmo_create` 的 16MiB 同口径（no-panic：巨 blob 的
/// 分配 abort 是 boot 期可触发 panic，超限降级为「无 rootfs」存活）。
const ROOTFS_MAX: usize = 16 << 20;

/// 全局分配器：内核 slab 堆（小对象走 slab，大对象走整页）。
#[global_allocator]
static ALLOC: SlabHeap = SlabHeap::new();

/// 把 `core::fmt` 格式串直接打到 UART（无缓冲、逐字节）——panic 消息/位置用。
struct UartWriter;

impl core::fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        berry_hal_aarch64::uart_puts(s.as_bytes());
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
    berry_hal_aarch64::halt();
}

/// 内核 Rust 入口（由 HAL boot asm 的 `br rust_main` 调用——2b-1 起高半入口）。
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配；首参 = 入口 x0（QEMU 传入的
/// DTB 物理地址——G-eco 入机侧，boot asm 的 x19 抢救）。
#[no_mangle]
pub extern "C" fn rust_main(dtb_pa: usize) -> ! {
    // 2b-1：注入 direct map 基址——必须先于任何 phys_to_virt 消费者（mm/heap）。
    berry_mm::va::set_kernel_va_base(berry_hal_aarch64::KERNEL_VA_BASE);
    berry_hal_aarch64::init();
    berry_hal_aarch64::uart_puts(b"BerryOS boot\n");

    // 2b-0：TTBR1 高半表自校验（RAM 魔数 + GIC PIDR2 双别名，见 HAL `mmu`）。
    // 不过 = 高半翻译未生效，2b-1 重链前必须在此暴露——panic 停机优于带病前进。
    if !berry_hal_aarch64::kva_ok() {
        panic!("KVA alias check failed: TTBR1 high-half not translating");
    }
    berry_hal_aarch64::uart_puts(b"KVA ok\n");

    // 2b-2：低半身份表退役——TTBR0 ← 内核空表 + `tlbi vmalle1is`。此后低半任何
    // 访问必然 fault（ktest ASPACE 用例正向断言）；UART/GIC MMIO 已走高半直映，
    // 内核对低半零依赖。回退开关见 HAL `mmu::RETIRE_IDENTITY`。
    berry_hal_aarch64::retire_low_half();

    // SAFETY: 启动单线程、`rust_main` 入口仅调用一次。
    unsafe { mm::init() };

    // G-eco 入机侧第一刀：rootfs（initrd 角色）装载，两源按序——
    // ① 入口 x0 的 DTB `/chosen` initrd 物理区间 → 高半直映拷出（真机/
    //    Linux boot 协议路径；QEMU 非 Linux ELF 直加载实测 x0=0、RAM 无
    //    DTB、`-initrd` 整体不加载——initrd/dtb/cmdline 机制全在 QEMU
    //    hw/arm/boot.c 的 is_linux 分支，2026-10 实证）；
    // ② QEMU fw_cfg 具名文件 `-fw_cfg name=opt/berry/rootfs,file=…`
    //    （目录自描述名字+大小、无魔法地址；justfile run-rootfs 配方）。
    // 装进 boot 期缓冲，下一步包只读 VMO 交 svcmgr（cpio/ELF 解析全在
    // 用户态——微内核只补「字节流变对象」原语）。无 rootfs = 正常降级
    // （不带 -fw_cfg 的既有 run/smoke 零扰动）。
    let rootfs: Option<(&[u8], alloc::vec::Vec<u8>)> = dtb::boot_probe(dtb_pa)
        .map(|(s, e)| {
            // SAFETY: boot_probe 已界检区间在 [RAM_BASE, RAM_TOP)——高半
            // 直映 L2 全覆盖，from_raw_parts 读取必落在已映射块内。
            let pa = phys_to_virt(s as usize) as *const u8;
            let bytes = unsafe { core::slice::from_raw_parts(pa, (e - s) as usize) };
            (b"dtb" as &[u8], bytes.to_vec())
        })
        .or_else(|| {
            use berry_hal_aarch64::fwcfg;
            let entry = fwcfg::find_file(fwcfg::ROOTFS_NAME)?;
            let size = entry.size as usize;
            // no-panic：与 vmo_create 16MiB 同口径——巨 blob 分配 abort 是
            // boot 期可触发 panic，超限/空文件一律降级为「无 rootfs」存活。
            if size == 0 || size > ROOTFS_MAX {
                return None;
            }
            let mut buf = alloc::vec![0u8; size];
            if !fwcfg::read_to(&entry, &mut buf) {
                return None;
            }
            Some((b"fwcfg" as &[u8], buf))
        });
    match rootfs {
        Some((src, buf)) => {
            let bytes = buf.len();
            // SAFETY: boot 单线程写入、此后只读；消费点随下一步「包只读
            // VMO 交 svcmgr」——不进任何并发路径。
            unsafe { *(&raw mut ROOTFS) = Some(buf) };
            berry_hal_aarch64::uart_puts(b"ROOTFS ok src=");
            berry_hal_aarch64::uart_puts(src);
            berry_hal_aarch64::uart_puts(b" bytes=");
            berry_hal_aarch64::uart_put_dec(bytes as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        }
        None => berry_hal_aarch64::uart_puts(b"ROOTFS absent\n"),
    }

    // 3-1：注册真 syscall 语义 handler（handle 三操作 + clock_read）进 HAL 表。
    // 必须先于 ktest::run（SYSCALL 用例）与一切用户进程。
    syscall::install();

    ktest::run();

    // Phase 2a Step 5：IRQ 派发钩子 + timer 管线（规律 tick，**不从 IRQ 调 switch**）。
    // SAFETY: EL1、启动单线程、仅一次：缓存 CNTFRQ/算 SLICE（不武装）。
    unsafe { berry_hal_aarch64::timer::init() };

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
    berry_sched::init();
    // 3-4c 遥测环：注册切换钩子（init 后、首个切换前——rust_main 尾部的
    // yield_now 即产生首条记录；watchdog 断言环在涨 + vct 单调）。
    telemetry::init();
    // 4-3b PI：注册 SLEEPQ 变更钩子（block 睡下后 / wake 摘队后全图重推；
    // 与 telemetry 同位——均须在首个切换前）。
    pi::init();
    berry_sched::spawn_as(task_a, TASK_A_MAGIC as usize, b"task_a", as_a.ttbr0_pa, as_a.asid);
    berry_sched::spawn_as(task_b, TASK_B_MAGIC as usize, b"task_b", as_b.ttbr0_pa, as_b.asid);

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

    // 4-5a：console 服务通道——**首个服务循环**（v1 留内核任务形态：用户态无
    // MMIO 映射，输出须经内核代劳，ADR「启动关键暂留内核」；4-5b svcmgr 落地
    // 后用户服务的通道面/循环形状与此同构）。**console 挂 leak 私有表**
    // （spawn_with_tag——4-4 随附「4-5 前置补丁」方向：常驻内核服务不睡共享
    // BOOTSTRAP_HT，PI 受捐判定只看自己的表）+ 稳态等待走 **wait_many
    // （WAIT_ANY）**——PI 双保险（私有表断错捐喷洒、哨兵断 donor 边，详见
    // spawn 处说明，勿改回 wait_one(endK)）。endU（发端 ObjRef）以 insert_ref
    // 落各用户表（RIGHT_SEND）——**用户打印全部改走真 IPC**（HAL 调试槽
    // 0xFE/0xFF 退役）。通道 opt-out（4-3c 五件套之 4）：固定优先级系统服务
    // 不借入调用方 intent。发送侧 -EBUSY/-EAGAIN 重试（用户模板，EL0 可抢占
    // 有界）、服务侧 wait READABLE → drain 循环。
    let ct: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let h_end_k = ct
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 console 收端"),
            ),
            handle::RIGHT_RECV,
        )
        .expect("main: console 收端进私有表");
    let h_end_u = syscall::bootstrap_table()
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 console 发端"),
            ),
            handle::RIGHT_SEND,
        )
        .expect("main: console 发端进内核侧表");
    let (pk, pu) = (
        ct.get_raw(h_end_k, handle::ObjType::Channel, 0)
            .expect("main: 取 console 收端 ObjRef")
            .0,
        syscall::bootstrap_table()
            .get_raw(h_end_u, handle::ObjType::Channel, 0)
            .expect("main: 取 console 发端 ObjRef")
            .0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { crate::channel::ChannelEnd::pair(pk, pu) };
    // 先于用户矩阵入队（RR 序早 → 早起服务）；消息纯字节不携带 handle。
    //
    // **稳态等待走 wait_many（WAIT_ANY）而非 wait_one(endK)——PI donor 边规避**
    // （4-5a 排障实证）：4-3b 的受捐判定对内核任务取**共享内核侧表**粒度
    // （ktest J 段「共享表粒度」固化的设计语义）——pi_client(32) 阻塞期间，
    // 凡睡着的内核任务（含当时挂共享表的 console）都「名义持有」其 peer 端而
    // 受捐 32；若 console 直睡 endK:READABLE（内核派生位），它自己又构成
    // donor 边（peer=endU、need SEND），把受捐额**级联给所有持 console 发端的
    // 用户进程**——waiter 冻结 32 后永不入睡，4-3a 严格优先把永不入睡的
    // prio-0 waker 永久饿死（sleeper 路径 2 无人唤、BLOCK ok 永不出现）。
    // 两道修法正交共存：①私有表（spawn_with_tag——受捐只看自己的表，错捐
    // 喷洒断根，fa bench/其他共享表 donor 不再波及 console）；②`donor_edge`
    // 对 WAIT_ANY 一律 None（4-2b「v2 收口」既有语义）——console 借
    // wait_many+哑 Event（ponger 4-2b 同款对照项习语）主动退出 v1 donor 面；
    // 高优 client 阻塞 console-full 时对本服务的真实捐赠不受影响（受捐方向
    // 不经 wait 对象）。
    let h_dummy_ev = ct
        .insert(
            KernelObj::Event(crate::event::Event::new()),
            handle::RIGHT_SIGNAL,
        )
        .expect("main: console 哑 Event 进私有表");
    // arg 打包（pi_server 同款）：高 32 位哑 Event、低 32 位 endK——均 console
    // 私有表本地值（SVC 经 caller_table tag≠0 分支落到本表解析）。
    berry_sched::spawn_with_tag(
        task_console,
        ((h_dummy_ev as usize) << 32) | h_end_k as usize,
        b"console",
        0,
        ct as *mut HandleTable as usize,
    );

    // 3-2：good 的 root Vmar 进**它自己的表** = 首能力（能力模型 §5「第一个
    // 能力从哪来」——用户拿自己的地址空间根，MAP|MANAGE|DUPLICATE）。handle
    // 作 spawn arg → 入口 x0 → good 的 3-2 验收链经真 syscall map 读写自己的
    // VMO（全链落在 good 自己的表里——per-process 拆分后其余进程看不见）。
    let good_vmar = up_good.vmar.take().expect("main: good vmar");
    let good_vmar_h = ht_good
        .insert(KernelObj::Vmar(good_vmar), RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE)
        .expect("main: 插 root Vmar 进 good 表");
    // 4-5a：console 发端 append 进 good 表（vmar 后首插 = 槽 2/代号 1）——
    // good 模板硬编码依赖，boot 断言即防线（漂移 → boot 期炸响）。
    let h_cu_good = ht_good
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 good 表");
    assert_eq!(h_cu_good, 0x0001_0002, "main: good 表 console 须槽 2/代号 1（模板硬编码）");

    // 4-5a：abuser/spawner 的 console 发端 = 各表**首插**（0x0001_0001）——入口
    // x0 改传之（原 0；两模板自带 handle 都在运行期创建，首插槽位无既有依赖）。
    let h_cu_abuser = ht_abuser
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 abuser 表");
    let h_cu_spawner = ht_spawner
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 spawner 表");
    berry_sched::spawn_user(user::USER_CODE_VA, good_vmar_h as usize, user::USER_STACK_TOP,
                          b"user_good", up_good.ttbr0_pa, up_good.asid, tag_good);
    berry_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_rogue_ptr", up_rogue_ptr.ttbr0_pa, up_rogue_ptr.asid, tag_rogue);
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_abuser as usize, user::USER_STACK_TOP,
                          b"user_abuser", up_abuser.ttbr0_pa, up_abuser.asid, tag_abuser);
    // 3-3/3-4 验收：spawner 经真 syscall 创建第二个进程；process_start 的
    // bootstrap 迁移把「礼物 VMO」handle 送进子进程表（子入口 x0 收到后读出
    // 魔数自证——跨进程能力流动，见 user.rs spawner 模板注释）。
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_spawner as usize, user::USER_STACK_TOP,
                          b"user_spawner", up_spawner.ttbr0_pa, up_spawner.asid, tag_spawner);
    berry_sched::spawn(user::watchdog, 0, b"watchdog");
    // 4-0 验收（`IPC设计.md` Phase 4 拆步表）：sleeper/waker 对打——超时自醒
    // （'T'）+ 条件唤醒（'K'）双路径全绿打 `BLOCK ok`（smoke 第 14 标记）。
    // ktest 在 sched::init 前跑不能睡，睡眠面只能走 boot spawn 活证。
    berry_sched::spawn(task_sleeper, 0, b"sleeper");
    berry_sched::spawn(task_waker, 0, b"waker");

    // 4-1 验收（`IPC设计.md` Phase 4 拆步表 4-1）：阻塞 object_wait_one——
    // waiter（EL0）三路（超时 -ETIMEDOUT / poll 命中回写全满足集 / 真睡被
    // signal 唤醒）+ waker2（内核任务）经真 SVC signal 内核侧表 handle。
    // 同一 Event **双表挂同一 ObjRef**（insert_ref）——signal 侧与 wait 侧各
    // 持自己表的 handle（能力跨表互通的活证，匹配键 = ObjRef 地址）。
    let up_waiter =
        user::UserProcess::build(mm::frames(), user::user_waiter_code(), user::USER_MAGIC);
    let ht_waiter: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    use handle::{ObjType, RIGHT_SIGNAL};
    let h_ev_kernel = syscall::bootstrap_table()
        .insert(KernelObj::Event(event::Event::new()), RIGHT_SIGNAL)
        .expect("main: Event 进内核侧表");
    let (ev_ptr, _) = syscall::bootstrap_table()
        .get_raw(h_ev_kernel, ObjType::Event, 0)
        .expect("main: 取 Event ObjRef");
    let h_ev_waiter = ht_waiter
        .insert_ref(ev_ptr, RIGHT_SIGNAL)
        .expect("main: Event 进 waiter 表（同一 ObjRef）");
    // 4-5a：console 发端 append（Event 后首插 = 槽 2/代号 1）——waiter 模板硬编码。
    let h_cu_waiter = ht_waiter
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 waiter 表");
    assert_eq!(h_cu_waiter, 0x0001_0002, "main: waiter 表 console 须槽 2/代号 1（模板硬编码）");
    berry_sched::spawn_user(user::USER_CODE_VA, h_ev_waiter as usize, user::USER_STACK_TOP,
                          b"user_waiter", up_waiter.ttbr0_pa, up_waiter.asid,
                          ht_waiter as *mut HandleTable as usize);
    berry_sched::spawn(task_waker2, h_ev_kernel as usize, b"waker2");

    // 4-2 验收（`IPC设计.md` Phase 4 拆步表 4-2）：pinger/ponger 两 EL0 进程经
    // channel 阻塞链对打——pinger（endA：RIGHT_SEND）send 灌到 -EBUSY →
    // wait_one(WRITABLE) 阻塞 → ponger（endB：RIGHT_RECV）recv 腾位唤醒 →
    // 补发成功打 `CHAIN ok`（smoke 第 16 标记）。两端各进**对方进程的表**
    //（`channel_create` 建对语义的 boot 手搭版：两表 insert + `pair` 互钉）。
    let up_pinger =
        user::UserProcess::build(mm::frames(), user::user_pinger_code(), user::USER_MAGIC);
    let up_ponger =
        user::UserProcess::build(mm::frames(), user::user_ponger_code(), user::USER_MAGIC);
    let ht_pinger: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_ponger: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    use crate::channel::ChannelEnd;
    use handle::{RIGHT_RECV, RIGHT_SEND};
    let h_end_a = ht_pinger
        .insert(
            KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endA")),
            RIGHT_SEND,
        )
        .expect("main: endA 进 pinger 表");
    // 4-5a：console 发端 append（endA 后首插 = 槽 2/代号 1）——pinger 模板硬编码。
    let h_cu_pinger = ht_pinger
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 pinger 表");
    assert_eq!(h_cu_pinger, 0x0001_0002, "main: pinger 表 console 须槽 2/代号 1（模板硬编码）");
    let h_end_b = ht_ponger
        .insert(
            KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endB")),
            RIGHT_RECV,
        )
        .expect("main: endB 进 ponger 表");
    let (pa, pb) = (
        ht_pinger.get_raw(h_end_a, ObjType::Channel, 0).expect("main: 取 endA ObjRef").0,
        ht_ponger.get_raw(h_end_b, ObjType::Channel, 0).expect("main: 取 endB ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pa, pb) };
    // 4-2b：哑 Event 进 ponger 表——wait_many 稳态等待的对照项（只入数组、
    // 永不 signal；等 1<<32 位 = 永不命中）。boot 直插定值断言：ponger 模板把
    // 该 handle 硬编为 0x0001_0002（表内第 2 插 = 槽 2/代号 1）——表序有变须同步
    // 模板（断言即防线，错值 → wait_many -EBADF → ponger 自陷被杀 → MANY 缺失）。
    let h_evt_ponger = ht_ponger
        .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
        .expect("main: 哑 Event 进 ponger 表");
    assert_eq!(
        h_evt_ponger, 0x0001_0002,
        "main: ponger 表哑 Event 须为槽 2/代号 1（ponger 模板硬编码依赖）"
    );
    // 4-5a：console 发端 append（endB + 哑 Event 后 = 槽 3/代号 1）——ponger 模板硬编码。
    let h_cu_ponger = ht_ponger
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 ponger 表");
    assert_eq!(h_cu_ponger, 0x0001_0003, "main: ponger 表 console 须槽 3/代号 1（模板硬编码）");
    berry_sched::spawn_user(user::USER_CODE_VA, h_end_a as usize, user::USER_STACK_TOP,
                          b"user_pinger", up_pinger.ttbr0_pa, up_pinger.asid,
                          ht_pinger as *mut HandleTable as usize);
    berry_sched::spawn_user(user::USER_CODE_VA, h_end_b as usize, user::USER_STACK_TOP,
                          b"user_ponger", up_ponger.ttbr0_pa, up_ponger.asid,
                          ht_ponger as *mut HandleTable as usize);

    // 4-3b 验收（`IPC设计.md` Phase 4 拆步表 4-3 前半）：PI 捐赠三任务链——
    // pi_client（prio 32，donor）send 灌满后 wait WRITABLE 阻塞；pi_server
    //（prio 0，持 recv 端 endS）阻塞在 Event E 上（受捐方——有界队列把 QNX
    //「双方同睡一个 channel」解耦，接收方持 handle 可睡任何对象，捐赠边靠
    // handle 表反查）；pi_probe 睡 5 slice 后断言 server 被捐到 eff=32、
    // signal E 放行，打 `PI ok`（smoke 第 18 标记）。三者全走真 SVC（EL1
    // 入口与 EL0 同权，3-1）；边闭合时序：client(32) 先跑睡 send-wait →
    // server 跑睡 E（block 钩子重推成边）→ probe 醒断言。
    //（RIGHT_SEND/RECV、ChannelEnd、ObjType、RIGHT_SIGNAL 均已在本函数
    // 更早的 use 里。）
    let h_end_c = syscall::bootstrap_table()
        .insert(KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endC")),
                RIGHT_SEND)
        .expect("main: endC 进内核侧表");
    let h_end_s = syscall::bootstrap_table()
        .insert(KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endS")),
                RIGHT_RECV)
        .expect("main: endS 进内核侧表");
    let (pc, ps) = (
        syscall::bootstrap_table().get_raw(h_end_c, ObjType::Channel, 0)
            .expect("main: 取 endC ObjRef").0,
        syscall::bootstrap_table().get_raw(h_end_s, ObjType::Channel, 0)
            .expect("main: 取 endS ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pc, ps) };
    let h_ev_pi = syscall::bootstrap_table()
        .insert(KernelObj::Event(event::Event::new()), RIGHT_SIGNAL)
        .expect("main: PI Event 进内核侧表");
    // arg 打包（spawn 只有单 usize 参）：server = (h_ev_pi << 32) | h_end_s；
    // probe = h_ev_pi；client = h_end_c。
    berry_sched::spawn(task_pi_server,
        ((h_ev_pi as usize) << 32) | h_end_s as usize, b"pi_server");
    berry_sched::spawn(task_pi_probe, h_ev_pi as usize, b"pi_probe");
    berry_sched::spawn_with(task_pi_client, h_end_c as usize, b"pi_client", 32);

    // 4-3c 验收（`IPC设计.md` Phase 4 拆步表 4-3 后半，intent 五件套落地）：
    // 六内核任务全真 SVC——driver(0x02) 三发 → mid(0x04) recv 借入 0x02 / 借入
    // 转发 hop+1 / opt-out 通道 recv 回落 → sinkA(0x03) 收转发件（hop==1 = mid
    // 借入转发的活证，eff==0x02）→ slow(0x04) 借入后睡 200ms 超每跳预算 50ms，
    // 转发降级（装自己的 0x04；借入字段保持——无回落事件）→ sinkB(0x03) 吃
    // 两条 boot 伪造超帽消息（hop 帽 / 链累计封顶，拒传播回落）→ probe 睡
    // 8 slice 遍历 SLEEPQ 断言四睡眠者字段 + 遥测三计数器，打 `INTENT ok`
    //（smoke 第 19 标记）后 exit。通道全走内核侧表（boot 手搭版，单核 IRQ
    // 屏蔽窗 = 无竞态）；CH3 置 options bit0（opt-out）；伪造消息绕过 send 直
    // 压收端队列——超帽样本的确定性触发（队列 boot 期必空不满）。
    let mk_it_chan = |no_intent: bool| -> (u32, u32) {
        let hs = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(no_intent).expect("main: 建 it send 端")),
                RIGHT_SEND)
            .expect("main: it send 端进内核侧表");
        let hr = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(no_intent).expect("main: 建 it recv 端")),
                RIGHT_RECV)
            .expect("main: it recv 端进内核侧表");
        let (ps, pr) = (
            syscall::bootstrap_table().get_raw(hs, ObjType::Channel, 0)
                .expect("main: 取 it send ObjRef").0,
            syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
                .expect("main: 取 it recv ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(ps, pr) };
        (hs, hr)
    };
    let (h_d1, h_m1) = mk_it_chan(false);   // CH1 driver → mid（首手 0x02）
    let (h_m2, h_a2) = mk_it_chan(false);   // CH2 mid → sinkA（借入转发 hop=1）
    let (h_d3, h_m3) = mk_it_chan(true);    // CH3 opt-out（bit0）——拒借回落面
    let (_h_x4a, h_b4a) = mk_it_chan(false); // CH4a 伪造 hop 帽样本（发送端闲置）
    let (_h_x4b, h_b4b) = mk_it_chan(false); // CH4b 伪造链累计封顶样本
    let (h_d5, h_l5) = mk_it_chan(false);   // CH5 driver → slow（借入后超预算）
    let (h_l6, h_b6) = mk_it_chan(false);   // CH6 slow → sinkB（降级转发件）
    // 伪造消息直压收端队列（LC 档 0x01 载荷——帽子拦下才不至抬档）：
    // hop=8 恰顶跳数帽；acc = 链封顶 +1ns 恰超累计帽。两道帽的 recv 结算
    // 拒传播路径由此确定性触发（不赌 send 快照路径）。
    let push_forged = |hr: u32, meta: berry_sched::MsgIntent| {
        let p = syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
            .expect("main: 取伪造收端 ObjRef").0;
        let mut m = crate::channel::Message::new();
        m.bytes[..4].copy_from_slice(b"ITK!");
        m.nbytes = 4;
        m.meta = meta;
        // SAFETY: 刚 get_raw 过（在表、类型 Channel）；boot 单线程。
        unsafe { (*p.as_ptr()).obj.as_channel_mut().unwrap().push(m) };
    };
    push_forged(h_b4a, berry_sched::MsgIntent {
        intent: berry_sched::INTENT_LATENCY_CRITICAL,
        hop: berry_sched::INTENT_HOP_MAX,
        acc_ns: 0,
    });
    push_forged(h_b4b, berry_sched::MsgIntent {
        intent: berry_sched::INTENT_LATENCY_CRITICAL,
        hop: 0,
        acc_ns: berry_sched::INTENT_CHAIN_BUDGET_NS + 1,
    });
    // handle 传递槽（spawn 单 usize 参装不下 12 个 handle）：boot 写入先于一切
    // 任务启动（单核屏蔽窗）。槽位：0=d1 1=m1 2=m2 3=a2 4=d3 5=m3 6=b4a
    // 7=b6 8=b4b 9=d5 10=l5 11=l6。
    {
        let slots = [h_d1, h_m1, h_m2, h_a2, h_d3, h_m3,
                     h_b4a, h_b6, h_b4b, h_d5, h_l5, h_l6];
        // SAFETY: boot 单线程写入，先于任何任务读。
        unsafe {
            for (i, &h) in slots.iter().enumerate() {
                *core::ptr::addr_of_mut!(INTENT_H).cast::<u32>().add(i) = h;
            }
        }
    }
    berry_sched::spawn_intent(task_intent_driver, 0, b"it_driver",
                              berry_sched::INTENT_INTERACTIVE);
    berry_sched::spawn_intent(task_intent_mid, 0, b"it_mid", berry_sched::INTENT_BACKGROUND);
    berry_sched::spawn_intent(task_intent_sink_a, 0, b"it_sinkA", berry_sched::INTENT_UTILITY);
    berry_sched::spawn_intent(task_intent_sink_b, 0, b"it_sinkB", berry_sched::INTENT_UTILITY);
    berry_sched::spawn_intent(task_intent_slow, 0, b"it_slow", berry_sched::INTENT_BACKGROUND);
    berry_sched::spawn(task_intent_probe, 0, b"it_probe");

    // ---- 4-4 fastpath/slowpath 分层验收（`IPC设计.md` §7）----
    // 专用点对点 bench 对（方法论语：量 cycle 别拿稳态矩阵当基线——console
    // 排队是噪声源）。fa_ping(16)/fa_pong(32) 各持一端 SEND|RECV 双权
    //（`mk_it_chan` 只授单端单权，不适用）。prio 16/32 > 矩阵/intent 全 0 →
    // bench 独占 CPU 至完成（ms 级；且此刻其余任务全 Ready 未睡 → SLEEPQ 只有
    // pong 短暂在队，PI 共享表粒度零外溢）。稳态每轮：pong 先睡 wait READABLE
    // → ping send = 检查链全过 fastpath + 直切（32>16）→ pong 醒 recv+回 send
    //（ping 尚 Ready 未睡 → miss 走 slowpath）→ pong 循环回 wait 阻塞 → 切回
    // ping → ping wait poll 即命中（reply 已在队）→ recv 验回显。每轮
    // hits=1/miss=1/direct=1；ping 收尾断言三计数器 + RT（CNTVCT 差均值，
    // 参考 cycle = ticks×1e9/CNTFRQ；TCG 先出量级、真机复测标注）打 `FAST ok`
    //（smoke 第 20 标记）后双双 exit。
    let mk_fast_chan = || -> (u32, u32) {
        let hs = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 fa A 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: fa A 端进内核侧表");
        let hr = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 fa B 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: fa B 端进内核侧表");
        let (ps, pr) = (
            syscall::bootstrap_table().get_raw(hs, ObjType::Channel, 0)
                .expect("main: 取 fa A ObjRef").0,
            syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
                .expect("main: 取 fa B ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(ps, pr) };
        (hs, hr)
    };
    let (h_fa, h_fb) = mk_fast_chan();
    berry_sched::spawn_with(task_fast_ponger, h_fb as usize, b"fa_pong", 32);
    berry_sched::spawn_with(task_fast_pinger, h_fa as usize, b"fa_ping", 16);

    // ---- 4-5 前置：常驻内核服务私有表使能自证（spawn_with_tag + caller_table
    // EL1 tag≠0 分支，修 4-3b 共享表粒度的错捐级联——常驻服务须挂私有表）----
    // svc_tag 持 leak 私有表里的通道端 + 拿共享表真 handle（fa B 端，fa 任务
    // 不 close 恒有效）作负向判别样本：poll-only 不入 SLEEPQ，PI/intent 零纠缠。
    {
        let st: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_own = st
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 tag A 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: tag A 端进私有表");
        let h_peer = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 tag B 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: tag B 端进内核侧表");
        let (pa, pb) = (
            st.get_raw(h_own, ObjType::Channel, 0).expect("main: 取 tag A ObjRef").0,
            syscall::bootstrap_table().get_raw(h_peer, ObjType::Channel, 0)
                .expect("main: 取 tag B ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(pa, pb) };
        // arg 打包双 handle：低 32 = 自己表的端，高 32 = 共享表的 fa B 端。
        let packed = h_own as usize | ((h_fb as usize) << 32);
        berry_sched::spawn_with_tag(task_svc_tag, packed, b"svc_tag", 0,
                                    st as *mut HandleTable as usize);
    }

    // ---- 4-5b 验收（`IPC设计.md` Phase 4 拆步表 4-5 后半）：svcmgr（EL0）经
    // 真 syscall 全链拉起 echo 服务（用户态进程）——里程碑「服务管理器拉起
    // 一个服务」。svcmgr 自身 boot 手搭（spawner 同形）；echo 由 svcmgr 运行期
    // 创建：process_create → vmo_create/commit/write(内嵌模板) → vmar_map →
    // channel_create → thread_create → process_start(end1 迁 echo 表)。验收
    // 往返（send ECHO_REQ → wait READABLE → recv 验回显）打 `SVC ok`（smoke
    // 第 21 标记）、稳态每圈 'v'（每个 'v' = 一次实证往返）。echo 静默（无
    // console handle——boot 无法预知运行期建表的 handle 值；断链由 svcmgr
    // 侧标记缺失/watchdog 错杀断言兜底）。PI：两 EL0 各持私有表、全 prio 0
    // ——互为 donor/受捐的捐赠额恒 0，无级联面。----
    let up_svcmgr =
        user::UserProcess::build(mm::frames(), user::user_svcmgr_code(), user::USER_MAGIC);
    let ht_svcmgr: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    // console 发端 = svcmgr 表**首插**（0x0001_0001）——入口 x0 传入（模板
    // 硬编码依赖，boot 断言拦表序漂移）。
    let h_cu_svcmgr = ht_svcmgr
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 svcmgr 表");
    assert_eq!(
        h_cu_svcmgr, 0x0001_0001,
        "main: svcmgr 表 console 须首插 0x0001_0001（模板硬编码）"
    );
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_svcmgr as usize, user::USER_STACK_TOP,
                          b"user_svcmgr", up_svcmgr.ttbr0_pa, up_svcmgr.asid,
                          ht_svcmgr as *mut HandleTable as usize);

    // ---- 4-6 B 半（AI 最小演示，`文档/4-6出口演示设计草案.md` B 半管线）：
    // 遥测 dump（`tele:` 行）→ host 侧 advisor.py（`工具链/`，工具非关卡）规则
    // 出建议 → 构建期回填注入表（演示级通道，v3 policy_bind 随 ADR #21 实验
    // 轨道再立运行时协议）→ 内核执法活证（越界 +200 被 clamp 到 ±64——apply/
    // weight_of 双点既有包络、热路径零改动，probe 读在队字段做机器断言）+
    // F9① 开销不变量最小兑现（bind 前后 bench RT 同量级，容差 2x——TCG 噪声
    // 界，-icount 精确版/真机 p99 随 FAST 线留 G 卡复测）。三张 leak 表：
    // tag_D（ai_demo 私有——睡 Event 等用户位，无 channel 受捐面）、tag_P
    //（bench 对两轮共用、bind 合法项 key）、tag_pr（probe 私有——**长命睡眠
    // 内核任务不睡共享表**，4-5 纪律：pi 窗错捐 eff 32 冻结随行、严格优先反
    // 噬 prio-0 轮转、probe 唤醒周期反把 demo 轮次推过自身重试预算，本步
    // bisect 实踩；表内仅 E_rel 一个 Event、无 channel 端点 → 零受捐面）。----
    let ht_ai: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_ab: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    // bench 通道对（fa 同款：双端各 SEND|RECV，两轮复用——每轮 send/recv 严格
    // 配对，轮末两队列必空回原始态）。
    let h_ab_a = ht_ab
        .insert(KernelObj::Channel(
            ChannelEnd::try_unpaired(false).expect("main: 建 ai A 端")),
            RIGHT_SEND | RIGHT_RECV)
        .expect("main: ai A 端进 bench 表");
    let h_ab_b = ht_ab
        .insert(KernelObj::Channel(
            ChannelEnd::try_unpaired(false).expect("main: 建 ai B 端")),
            RIGHT_SEND | RIGHT_RECV)
        .expect("main: ai B 端进 bench 表");
    let (pab_a, pab_b) = (
        ht_ab.get_raw(h_ab_a, ObjType::Channel, 0).expect("main: 取 ai A ObjRef").0,
        ht_ab.get_raw(h_ab_b, ObjType::Channel, 0).expect("main: 取 ai B ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pab_a, pab_b) };
    // E_done（bench 完成事件）：demo 表原件（等 1<<34）+ bench 表 insert_ref
    //（pinger 收尾 signal 用——跨表同一 ObjRef，匹配键互通）。
    let h_done_d = ht_ai
        .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
        .expect("main: E_done 进 demo 表");
    let p_done = ht_ai
        .get_raw(h_done_d, ObjType::Event, 0)
        .expect("main: 取 E_done ObjRef").0;
    let h_done_p = ht_ab
        .insert_ref(p_done, handle::RIGHT_SIGNAL)
        .expect("main: E_done 落 bench 表");
    // E_rel（probe 放行事件）：demo 表原件（等 1<<36）+ probe 私有表
    // insert_ref（probe signal 用——跨表同一 ObjRef，匹配键互通）。
    let h_rel_d = ht_ai
        .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
        .expect("main: E_rel 进 demo 表");
    let p_rel = ht_ai
        .get_raw(h_rel_d, ObjType::Event, 0)
        .expect("main: 取 E_rel ObjRef").0;
    let ht_pr: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let h_rel_p = ht_pr
        .insert_ref(p_rel, handle::RIGHT_SIGNAL)
        .expect("main: E_rel 落 probe 表");
    // demo 上下文（spawn arg 单指针携带；tag 值是运行期表指针，注入表只能
    // 运行期构造——key 编译期不可知）。
    let ai_ctx: &'static AiDemoCtx =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(AiDemoCtx {
            h_done: h_done_d,
            h_rel: h_rel_d,
            h_ping_arg: h_ab_a as usize | (h_done_p as usize) << 32,
            h_pong_arg: h_ab_b as usize,
            tag_p: ht_ab as *mut HandleTable as usize,
            tag_d: ht_ai as *mut HandleTable as usize,
        }));
    berry_sched::spawn_with_tag(task_ai_demo, ai_ctx as *const AiDemoCtx as usize,
                                b"ai_demo", 0, ht_ai as *mut HandleTable as usize);
    berry_sched::spawn_with_tag(task_ai_probe, h_rel_p as usize, b"ai_probe", 0,
                                ht_pr as *mut HandleTable as usize);

    // 注册回调（HAL→sched 反转，避循环依赖；均须在 enable_irq / 首个用户任务被
    // 调度之前）。IRQ 槽驱动抢占；用户故障槽驱动 G1.5 kill。
    berry_hal_aarch64::set_irq_dispatch(berry_sched::on_irq_tick);
    berry_hal_aarch64::set_user_fault_exit(berry_sched::user_fault_exit);
    // 武装首个时间片 + 解蔽 IRQ：之后 timer 每 slice（10Hz）落 exception_irq → on_irq_tick。
    // SAFETY: 写 CNTV_CTL/CVAL（绝对截止）；EL1。arm_slice 设 ENABLE+不屏蔽（0b01）。
    unsafe { berry_hal_aarch64::timer::arm_slice() };
    berry_hal_aarch64::enable_irq();
    berry_sched::yield_now(); // idle(main) 让出 → task_a；timer 每 slice 抢占切到 task_b → A/B 交替。
    // 不可达（忙循环任务不退出）；防御兜底：若被切回则进 tickless idle。
    berry_sched::idle_loop();
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
            berry_hal_aarch64::uart_puts(b"A");
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
            berry_hal_aarch64::uart_puts(b"B");
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

/// 4-0 睡眠/唤醒验收的等待对象标识（静态地址 = `wake_matching` 的 obj 匹配键，
/// 不透明 token——4-1 起该语义交给 Event/Channel 的 ObjRef 地址）。
static SLEEP_OBJ: u8 = 0;

/// sleeper 双路径全过标志（唤醒环 watchdog 断言的门槛——置位即两条 boot
/// 唤醒记录已落 `telemetry::WAKE_RING`，断言才不赌 boot 时序）。单核 v1
/// Relaxed 足够（读写都在屏蔽窗/单核串行语境）。
pub static BLOCK_BOOT_DONE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// 4-0 sleeper：两条唤醒路径各验一遍，全绿打 `BLOCK ok`。
///
/// - 路径 1（超时自醒 'T'）：纯 sleep 1 slice——醒因必 Timeout 且实际醒来
///   ≥ deadline（tick 粗粒度扫描：到期后最迟一个 slice 被摘）。
/// - 路径 2（条件唤醒 'K'）：挂 SLEEP_OBJ 无限等（deadline=MAX），由 waker 的
///   `wake_matching` 摘醒——醒因必 Signaled（单核屏蔽窗原子：唤醒方只在匹配
///   时摘队，不存在虚假唤醒；醒后重验条件的兜底语义归 4-1 调用方）。
///
/// block_current 契约：内核任务显式 disable → block → 续行后 enable（对称
/// 包络，同 yield_now 模式；SVC 语境无此步——eret 自然解蔽）。
extern "C" fn task_sleeper(_arg: usize) -> ! {
    use berry_sched::WakeReason;
    let slice = berry_hal_aarch64::timer::slice_cycles();
    // 路径 1：超时自醒。
    berry_hal_aarch64::disable_irq();
    let t0 = berry_hal_aarch64::timer::cntvct();
    let r = berry_sched::block_current(0, 0, t0 + slice);
    berry_hal_aarch64::enable_irq();
    let elapsed = berry_hal_aarch64::timer::cntvct() - t0;
    assert!(r == WakeReason::Timeout, "sleeper: 超时路径醒因非 Timeout");
    assert!(elapsed >= slice, "sleeper: 超时醒来早于 deadline");
    berry_hal_aarch64::uart_puts(b"T");
    // 路径 2：条件唤醒（无限等——无哨兵，deadline=MAX 等价）。**只等用户位**
    // 1<<33（内核永不置位）——4-3b 起 `donor_edge` 的不变量是「请求位含内核
    // 派生位（READABLE/WRITABLE）⟹ wait_obj 是真 channel ObjRef」：SLEEP_OBJ
    // 是不透明 token（u8 静态，非对象），带派生位睡它 = 让 PI 图解引用垃圾
    // 指针（曾以 !0 触发 pi.rs 对齐 panic 实证）。
    berry_hal_aarch64::disable_irq();
    let r2 = berry_sched::block_current(&SLEEP_OBJ as *const u8 as usize, 1 << 33, u64::MAX);
    berry_hal_aarch64::enable_irq();
    assert!(r2 == WakeReason::Signaled, "sleeper: 条件路径醒因非 Signaled");
    // 双路径全过——置门槛放行 watchdog 唤醒环断言（此刻两条 boot 唤醒记录必
    // 已落环：钩子在唤醒时刻发射，先于 sleeper 续行到这里）。
    BLOCK_BOOT_DONE.store(true, core::sync::atomic::Ordering::Relaxed);
    berry_hal_aarch64::uart_puts(b"K BLOCK ok\n");
    berry_sched::exit_current()
}

/// 4-0 waker：轮询 `wake_matching(SLEEP_OBJ, 1<<33)` 直到 sleeper 双路径全过。
/// 未挂上时零副作用——单核屏蔽窗原子保证无丢唤醒：要么本循环先扫（sleeper
/// 稍后挂上，下轮摘到）、要么 sleeper 已挂（本轮即摘）。轮询体 IRQ 基本解蔽
/// → tick 正常抢占 → sleeper 得以上 CPU 挂队。（置位集与 sleeper 的请求位
/// 同步收窄到用户位 1<<33——理由见路径 2 注释。）
///
/// **收工判据 = sleeper 侧 `BLOCK_BOOT_DONE`**（4-5a 修正）：4-2b 起
/// `wake_matching` 对 `WAIT_ANY` 哨兵睡眠者（wait_many）「任何对象、任何
/// 非零位」都命中——ponger 稳态全程挂哨兵，本循环的定向轮询会误摘它一票，
/// 返回计数混入哨兵误醒（可能 1 也可能 ≥2），不再可信。改看 sleeper 置的
/// 完成标志：定向唤醒语义不变（SLEEP_OBJ+1<<33 的精确匹配只可能来自本循
/// 环）；被误醒的哨兵者由其重验循环自行回睡（4-2b 设计内行为，无害）。
extern "C" fn task_waker(_arg: usize) -> ! {
    while !BLOCK_BOOT_DONE.load(core::sync::atomic::Ordering::Relaxed) {
        berry_hal_aarch64::disable_irq();
        berry_sched::wake_matching(&SLEEP_OBJ as *const u8 as usize, 1 << 33);
        berry_hal_aarch64::enable_irq();
        // 降频小忙转（可被 tick 抢占）。
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }
    berry_sched::exit_current()
}

/// 4-1 waker2 的 EL1 真 SVC 往返（ktest `svc6` 同款裁 4 参）：x0=nr（进）/
/// ret（出）、x1..x3=参。走 current-EL SVC 向量 → HAL dispatch → 语义层——
/// 全真 syscall 链（非直调 handler）。
fn svc4(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同步异常；dispatch 只回写 x0。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack, preserves_flags),
        );
    }
    ret as i64
}

/// 4-1 waker2：等 waiter 挂上路 3 长睡（3 slice 时延），经**真 SVC** 对内核侧
/// 表 handle 发 `object_signal(h, 0, U3)`——唤醒链全真：EL1 SVC 入口 →
/// 语义层 signal → `wake_matching` → waiter 醒来重验命中。再睡 1 slice 让
/// waiter 跑完三路进稳态，打关卡标记 `WAIT ok`（smoke 第 15）。waiter 断链
/// 被杀 → 杀数超 1 → watchdog 错杀断言拦下（无需本任务二次确认）。
extern "C" fn task_waker2(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_kernel = arg as u32;
    // SAFETY: 只读 CNTFRQ / slice 常量（timer::init 后恒有效）。
    let freq = berry_hal_aarch64::timer::cntfrq();
    assert!(freq != 0, "waker2: CNTFRQ 异常");
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；now 为本任务私有栈变量（内核高半，copy_out 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "waker2: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 3 * slice_ns, 0, 0),
        0,
        "waker2: thread_sleep 3 slice"
    );
    // waiter 路 3 已挂队（U3）→ 置位唤醒（清 0、置 U3=1<<35——4-2 位空间起
    // Event 用户位在高 32 半）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_kernel as u64, 0, 1 << 35),
        0,
        "waker2: object_signal U3"
    );
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "waker2: clock_read 2");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + slice_ns, 0, 0),
        0,
        "waker2: thread_sleep 1 slice（让 waiter 收尾）"
    );
    berry_hal_aarch64::uart_puts(b"W WAIT ok\n");
    berry_sched::exit_current()
}

/// 4-3b 三任务的 EL1 真 SVC 往返（waker2 `svc4` 同款扩到 6 参）：channel_send/
/// recv（5 参）与 object_wait_one（4 参）的载体，x0=nr（进）/ret（出）。
fn svc6(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同步异常；dispatch 只回写 x0（ktest `svc6` 同款）。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            in("x6") a6,
            options(nostack, preserves_flags),
        );
    }
    ret as i64
}

/// 4-3b pi_server（prio 0，受捐方）：先挂 Event E（用户位 1<<35，100s 帽）睡
/// ——client 的 8 条 PING 此刻已在 endS 队列堆着（server 没人 recv），本任务
/// 的「处理慢」由睡 E 模拟（下游依赖）。被 probe 的 signal 唤醒后醒来 recv
/// ×8 腾位（第一条即唤醒 client 的 WRITABLE）→ exit。醒来时 `pi_inherited`
/// 冻结随行（「server 处理消息期间持有」的 v1 形态——冻结面由 ktest J 段
/// 机器断言，本任务只走功能闭环）。
extern "C" fn task_pi_server(arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let h_end_s = arg as u32;
    let h_ev = (arg >> 32) as u32;
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；now 为本任务私有栈变量（内核高半，copy_out 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_server: clock_read");
    let mut observed = 0u64;
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_ev as u64, 1 << 35,
             now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "pi_server: wait E（被 probe signal 唤醒）"
    );
    // 醒来（冻结 boost 随行）：清空 endS 队列——第一条 recv 即 wake client。
    let mut buf = [0u8; 8];
    let mut hs = [0u32; 4];
    let (mut n, mut nh): (u64, u64);
    for i in 0..8 {
        // 容量 in-out：每次 recv 前重置（实际值出参回写）。
        n = 8;
        nh = 4;
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, h_end_s as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, hs.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "pi_server: recv 第 {} 条（腾位）", i + 1
        );
    }
    berry_sched::exit_current()
}

/// 4-3b pi_probe（prio 0，验收司机）：睡 5 slice 等 server/client 双双挂队
/// （client(32) 先跑睡 send-wait、server 随后睡 E——block 钩子重推成边），醒
/// 来遍历 SLEEPQ 断言捐赠字段（server pi_inherited==32 且 eff==32；client 自身
/// eff==32）→ signal E 放行 → 打 `PI ok`（smoke 第 18 标记）→ exit。字段断言
/// 即机器关卡；「醒来冻结随行 + 4-3a 严格优先」的行为面交 ktest J 段与既有
/// POLICY 面（G 卡用例的延迟上界验收留正式关卡）。
extern "C" fn task_pi_probe(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_ev = arg as u32;
    // SAFETY: 同 pi_server。
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 同 pi_server。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_probe: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 5 * slice_ns, 0, 0),
        0,
        "pi_probe: thread_sleep 5 slice（等双睡者挂队）"
    );
    // 断言捐赠图：server（睡 E）受捐 32；client（睡 endC:WRITABLE）自身 32。
    let (mut saw_server, mut saw_client) = (false, false);
    berry_sched::for_each_sleeper(|t| {
        if t.name == b"pi_server" {
            saw_server = true;
            assert_eq!(t.pi_inherited, 32, "pi_probe: server 须受捐到 32");
            assert_eq!(t.prio_effective, 32, "pi_probe: server eff 须被抬到 32");
        }
        if t.name == b"pi_client" {
            saw_client = true;
            assert_eq!(t.prio_effective, 32, "pi_probe: client(donor) 自身 eff");
        }
    });
    assert!(saw_server && saw_client, "pi_probe: 双睡眠者未同时在线（时序未达）");
    // 放行：signal E → server 醒（冻结 boost）→ recv 腾位 → client 醒。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_ev as u64, 0, 1 << 35),
        0,
        "pi_probe: signal E 放行"
    );
    berry_hal_aarch64::uart_puts(b"I PI ok\n");
    berry_sched::exit_current()
}

/// 4-3b pi_client（prio 32，donor）：send×8 灌满 endS 队列（每条成功即
/// wake peer READABLE——无人等，无害）→ 第 9 条 -EBUSY → wait_one(endC,
/// WRITABLE, 100s 帽) 睡——**block 钩子触发 PI 全图重推**（server 尚未睡 E
/// 时无边；server 睡 E 的 block 重推才闭合边——两种时序都收敛到同一图）。
/// server recv 腾位唤醒后：补发一条须成功（腾位自证）→ exit。
extern "C" fn task_pi_client(arg: usize) -> ! {
    use crate::handle::ERR_BUSY;
    use crate::syscall::{NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let h_end_c = arg as u32;
    let msg = *b"PI!\0";
    let mut saw_full = false;
    for i in 0..9 {
        let r = svc6(NR_CHANNEL_SEND as u64, h_end_c as u64,
                     msg.as_ptr() as u64, 4, 0, 0, 0);
        if r == ERR_BUSY as i64 {
            saw_full = true; // 第 i+1 条撞满（i==8 为正常路径；早满也收敛）
            break;
        }
        assert_eq!(r, 0, "pi_client: send 第 {} 条", i + 1);
    }
    assert!(saw_full, "pi_client: 9 条未撞满（队列深 8——异常）");
    let mut now: u64 = 0;
    // SAFETY: 同 pi_server。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_client: clock_read");
    let mut observed = 0u64;
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_end_c as u64,
             crate::handle::SIGNAL_WRITABLE, now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "pi_client: wait WRITABLE（server recv 腾位唤醒）"
    );
    assert!(
        observed & crate::handle::SIGNAL_WRITABLE != 0,
        "pi_client: observed 须含 WRITABLE"
    );
    // 腾位自证：补发一条须成功。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, h_end_c as u64, msg.as_ptr() as u64, 4, 0, 0, 0),
        0,
        "pi_client: 腾位后补发"
    );
    berry_sched::exit_current()
}

// ---- 4-5a console 服务（首个服务循环，`IPC设计.md` Phase 4 拆步表 4-5 前半）----

/// console 已落串口字节数——4-5a 起 watchdog 金丝雀（替 0xFF 调试槽的
/// USER_PUTCNT：打印异步化后「发出」与「落串口」分离，活证取**服务侧**计数）。
static CONSOLE_DRAINED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 读 console 服务排水计数（watchdog 用）。
pub fn console_drained() -> u64 {
    CONSOLE_DRAINED.load(core::sync::atomic::Ordering::Relaxed)
}

/// 4-5a console 服务任务：**首个服务循环**——wait_many [endK:READABLE, 哑
/// Event:1<<32]（任一命中；10s 心跳帽，空等超时非错误、回环重等）→ drain
/// recv 排空本端队列 → 逐字节 UART + 计数。全真 SVC 链（EL1 入口与 EL0 同权，
/// 3-1）；消息纯字节、不携带 handle（recv cap 0）。opt-out 通道 = 固定优先级
/// 系统服务（recv 不结算 intent 借入——4-3c 五件套之 4 的常驻消费面）。
///
/// **为何 wait_many 而非 wait_one(endK)**：多对象等待者挂 WAIT_ANY 哨兵 →
/// `pi::donor_edge` 返回 None（4-2b「v2 收口」）——console 退出 v1 PI donor
/// 面，否则（共享内核侧表的受捐粒度下）它把受捐额级联给全体持发端用户、
/// 冻结 boost + 4-3a 严格优先 = 永不入睡任务的饥饿死锁（见 boot spawn 处
/// 完整因果链）。哑 Event 永不 signal → 命中必 idx==0（idx==1 即自陷被
/// watchdog 错杀断言拦下）。console 是常驻服务**不退出**（与 sleeper/waker
/// 等验收任务不同）；4-5b svcmgr 落地后用户态服务与此同构。
extern "C" fn task_console(arg: usize) -> ! {
    use crate::handle::{ERR_AGAIN, ERR_TIMEDOUT, SIGNAL_READABLE};
    use crate::syscall::{NR_CHANNEL_RECV, NR_CLOCK_READ, NR_OBJECT_WAIT_MANY};
    let h_end_k = arg as u32;
    let h_dummy = (arg >> 32) as u32;
    // wait_many 两数组（栈上、EL1 内核高半——copy_from_user 的 EL1 分支收）。
    let hs = [h_end_k, h_dummy];
    let sigs = [SIGNAL_READABLE, 1u64 << 32];
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；栈变量私有（内核高半，copy 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    let (mut idx, mut observed): (u64, u64) = (0, 0);
    let (pidx, pobs) = (
        core::ptr::addr_of_mut!(idx) as u64,
        core::ptr::addr_of_mut!(observed) as u64,
    );
    let mut buf = [0u8; 256];
    let (mut n, mut nh): (u64, u64);
    loop {
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "console: clock_read");
        let r = svc6(
            NR_OBJECT_WAIT_MANY as u64,
            hs.as_ptr() as u64,
            sigs.as_ptr() as u64,
            2,
            now + 10 * 1_000_000_000,
            pidx,
            pobs,
        );
        assert!(
            r == 0 || r == ERR_TIMEDOUT as i64,
            "console: wait_many 异常 ret={}",
            r
        );
        if r == 0 {
            assert_eq!(idx, 0, "console: 须命中 endK（哑 Event 永不置位）");
        }
        // drain：排空本端队列（发送侧重试 + 深度 8 → 批量有界）。
        loop {
            n = 256;
            nh = 0;
            let r2 = svc6(
                NR_CHANNEL_RECV as u64,
                h_end_k as u64,
                buf.as_mut_ptr() as u64,
                core::ptr::addr_of_mut!(n) as u64,
                0,
                core::ptr::addr_of_mut!(nh) as u64,
                0,
            );
            if r2 == ERR_AGAIN as i64 {
                break; // 排空——回外层阻塞等
            }
            assert_eq!(r2, 0, "console: recv 异常");
            for i in 0..n as usize {
                berry_hal_aarch64::uart_putc(buf[i]);
                CONSOLE_DRAINED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

// ---- 4-3c intent 验收六任务（全真 SVC；ADR #42 五件套，`berry_sched::intent`）----

/// 4-3c handle 传递槽（spawn 只有单 usize 参——多 handle 走静态槽传递）。
/// boot 在 spawn 前写入、先于一切任务读（单核 IRQ 屏蔽窗 = 无竞态）；槽位
/// 表见 main 的 4-3c 段注释。
static mut INTENT_H: [u32; 12] = [0; 12];

/// 读 4-3c 传递槽（任务侧）。
fn it_h(i: usize) -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(INTENT_H).cast::<u32>().add(i) }
}

/// it_driver（declared INTERACTIVE 0x02，首手）：三发——CH1 借出 0x02 给
/// mid、CH3 opt-out 通道照发（拒借在收方结算侧）、CH5 借出 0x02 给 slow。
/// driver 先入队（spawn FIFO）→ 收方 recv 必不空。发完即退。
extern "C" fn task_intent_driver(_arg: usize) -> ! {
    use crate::syscall::NR_CHANNEL_SEND;
    let msg = *b"ITK!\0\0\0\0";
    for &slot in &[0usize, 4, 9] {
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, it_h(slot) as u64,
                 msg.as_ptr() as u64, 8, 0, 0, 0),
            0,
            "it_driver: send 槽 {} 须成功（driver 先于收方入队）", slot
        );
    }
    berry_sched::exit_current()
}

/// it_mid（declared BACKGROUND 0x04）：recv CH1 借入 0x02（hop=0 结算）→
/// send CH2 借入转发（快照 hop+1/acc+持有段——sinkA 的 hop==1 即本跳活证）→
/// recv CH3（opt-out 通道：不结算，回落 declared 0x04，含清掉他途借入）。
/// 此后纯睡不醒（deadline=MAX），留 SLEEPQ 供 probe 断言。
extern "C" fn task_intent_mid(_arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(1) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_mid: recv CH1（借入 0x02）"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, it_h(2) as u64, buf.as_ptr() as u64, 8, 0, 0, 0),
        0,
        "it_mid: 转发 CH2（借入快照 hop+1）"
    );
    n = 8; // 容量 in-out 每次 recv 前重置
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(5) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_mid: recv CH3（opt-out 拒借回落）"
    );
    berry_sched::block_current(0, 0, u64::MAX); // 纯睡永不醒：留 SLEEPQ 供 probe 断言
    unreachable!("it_mid: u64::MAX 不到期")
}

/// it_sinkA（declared UTILITY 0x03）：recv CH2——mid 借入转发的消息
///（intent 0x02 / hop 1 / acc>0）结算借入生效。此后纯睡，字段定格供 probe
/// 断言（eff==0x02、hop==1）。
extern "C" fn task_intent_sink_a(_arg: usize) -> ! {
    use crate::syscall::NR_CHANNEL_RECV;
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(3) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkA: recv CH2（两跳链尾）"
    );
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_sinkA: u64::MAX 不到期")
}

/// it_sinkB（declared UTILITY 0x03）：三连收——CH4a 伪造 hop 帽（拒传播回
/// 落）→ wait_one(CH6, READABLE) 阻塞等 slow 睡 200ms 后发出（4-2 阻塞链
/// 复用，不赌时序）→ recv CH6（slow 的降级转发件，Own 不计数）→ recv CH4b
/// 伪造链累计封顶（拒传播回落）。终态 eff==declared 0x03。
extern "C" fn task_intent_sink_b(_arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(6) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH4a（hop 帽拒传播）"
    );
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_sinkB: clock_read");
    let mut observed = 0u64;
    // SAFETY: 同上。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, it_h(7) as u64, SIGNAL_READABLE,
             now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "it_sinkB: wait CH6 READABLE（slow 发出唤醒）"
    );
    assert!(observed & SIGNAL_READABLE != 0, "it_sinkB: observed 须含 READABLE");
    n = 8;
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(7) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH6（降级转发件）"
    );
    n = 8;
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(8) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH4b（链累计封顶拒传播）"
    );
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_sinkB: u64::MAX 不到期")
}

/// it_slow（declared BACKGROUND 0x04）：recv CH5 借入 0x02 → 睡 200ms（超
/// 每跳预算 50ms）→ send CH6：快照降级——借来的 0x02 不再传播，装自己的
/// declared 0x04（budget_capped 计数 + trace 行）。**借入字段保持**（无回
/// 落事件——转发降级不清自身借入，probe 断言 eff==0x02）。
extern "C" fn task_intent_slow(_arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_THREAD_SLEEP};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(10) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_slow: recv CH5（借入 0x02）"
    );
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_slow: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 200_000_000, 0, 0),
        0,
        "it_slow: 睡 200ms（超每跳预算 50ms）"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, it_h(11) as u64, buf.as_ptr() as u64, 8, 0, 0, 0),
        0,
        "it_slow: send CH6（降级转发）"
    );
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_slow: u64::MAX 不到期")
}

/// it_probe（验收司机）：睡 8 slice（slice_ns 换算同 waker2——clock 域
/// 确定性）等全链落定 → 遍历 SLEEPQ 断言四睡眠者 intent 字段 + 遥测三计数
/// 器（borrows≥3 / hop_capped≥1 / budget_capped≥2——send 降级 + recv 封顶
/// 两路）→ 打 `INTENT ok`（smoke 第 19 标记）→ exit。字段即机器关卡：借入
/// 生效 / opt-out 回落 / hop==1 转发活证 / 双帽拒传播 / 降级转发不清借入，
/// 全链一次断言。
extern "C" fn task_intent_probe(_arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_THREAD_SLEEP};
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_probe: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 8 * slice_ns, 0, 0),
        0,
        "it_probe: thread_sleep 8 slice（等全链落定）"
    );
    let (mut saw_mid, mut saw_a, mut saw_b, mut saw_slow) = (false, false, false, false);
    berry_sched::for_each_sleeper(|t| {
        match t.name {
            b"it_mid" => {
                saw_mid = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_BACKGROUND,
                           "it_probe: mid 须 opt-out 回落 0x04");
            }
            b"it_sinkA" => {
                saw_a = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_INTERACTIVE,
                           "it_probe: sinkA 须借入 0x02");
                assert_eq!(t.intent_hop, 1,
                           "it_probe: sinkA hop==1（mid 借入转发活证）");
            }
            b"it_sinkB" => {
                saw_b = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_UTILITY,
                           "it_probe: sinkB 须双帽+降级后回落 0x03");
            }
            b"it_slow" => {
                saw_slow = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_INTERACTIVE,
                           "it_probe: slow 借入须保持（转发降级不清借入）");
            }
            _ => {}
        }
    });
    assert!(saw_mid && saw_a && saw_b && saw_slow,
           "it_probe: 四睡眠者未同时在线（时序未达）");
    assert!(crate::telemetry::intent_borrows() >= 3,
           "it_probe: 借入计数须 ≥3（mid/sinkA/slow 三次结算）");
    assert!(crate::telemetry::intent_hop_capped_total() >= 1,
           "it_probe: hop 帽计数须 ≥1（CH4a）");
    assert!(crate::telemetry::intent_budget_capped_total() >= 2,
           "it_probe: 时间帽计数须 ≥2（slow send 降级 + CH4b 结算封顶）");
    berry_hal_aarch64::uart_puts(b"I INTENT ok\n");
    berry_sched::exit_current()
}

// ---- 4-4 fastpath/slowpath 验收 bench（表见 main 的 4-4 段注释）----

/// bench 轮数：128 轮 × 每轮 µs–ms 级 = 整体 ms 级完成（prio 16/32 独占，
/// 先于一切矩阵/intent 任务跑完退出）。
const FAST_ROUNDS: u64 = 128;

/// fa_pong（prio 32，持 B 端）：每轮 wait READABLE 阻塞（正常路径被 ping 的
/// send fastpath 定点唤醒 + 直切上 CPU）→ recv 验 PING → 回 PONG（此刻 ping
/// 尚 Ready 未睡 → 这条 send 恒走 slowpath，bench 的 miss 来源）。N 轮后
/// exit——末轮回件后即退，不在 SLEEPQ 留尾。
extern "C" fn task_fast_ponger(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let hb = arg as u32;
    let mut buf = [0u8; 8];
    for _ in 0..FAST_ROUNDS {
        let mut now: u64 = 0;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pnow = core::ptr::addr_of_mut!(now) as u64;
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "fa_pong: clock_read");
        let mut observed = 0u64;
        // SAFETY: 同上。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // 10s 帽兜底：断链（fastpath 漏醒）则到点 -ETIMEDOUT 断言失败 = smoke
        // FAIL（fail loud，不静默挂死拖 DEADLINE_BOOT）。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, hb as u64, SIGNAL_READABLE,
                 now + 10_000_000_000, pobs, 0, 0),
            0,
            "fa_pong: wait READABLE（ping send 定点唤醒）"
        );
        assert!(observed & SIGNAL_READABLE != 0, "fa_pong: observed 须含 READABLE");
        let (mut n, mut nh): (u64, u64) = (8, 0); // 容量 in-out（recv 前备好）
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, hb as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "fa_pong: recv PING"
        );
        assert!(buf[..4] == *b"PING", "fa_pong: 载荷须 PING");
        buf[..4].copy_from_slice(b"PONG"); // 回件换文（PING→PONG）
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hb as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "fa_pong: 回 PONG"
        );
    }
    berry_sched::exit_current()
}

/// fa_ping（prio 16，持 A 端）：每轮计 t0 → send PING（fastpath 检查链全过 →
/// 直切到 pong）→ 被切回（pong 回件后 block）→ wait poll 即命中 PONG 在队
///（deadline=0 纯 poll——断链 -EAGAIN 秒失败）→ recv 验回显 → 累计 RT。收尾
/// 断言三计数器（hits/miss/direct，弹性 N-2 容 tick 边角）+ 打 `FAST ok`
///（smoke 第 20 标记）与 RT 数字（ticks 均值 + 1GHz 参考 cycle = ticks×1e9/
/// CNTFRQ——TCG 先出量级，真机复测标注）。
extern "C" fn task_fast_pinger(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_OBJECT_WAIT_ONE};
    let ha = arg as u32;
    let freq = berry_hal_aarch64::timer::cntfrq();
    let mut buf = [0u8; 8];
    let mut rt_ticks: u64 = 0;
    for _ in 0..FAST_ROUNDS {
        buf[..4].copy_from_slice(b"PING");
        let t0 = berry_hal_aarch64::timer::cntvct();
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, ha as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "fa_ping: send PING（fastpath：pong 在睡等 READABLE）"
        );
        let mut observed = 0u64;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // poll-first 命中：pong 回件先于切回 ping 落队（确定性时序），零阻塞。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, ha as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
            0,
            "fa_ping: wait poll 命中（PONG 已在队；断链 -EAGAIN 即 FAIL）"
        );
        let (mut n, mut nh): (u64, u64) = (8, 0); // 容量 in-out（recv 前备好）
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, ha as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "fa_ping: recv PONG"
        );
        assert!(buf[..4] == *b"PONG", "fa_ping: 载荷须 PONG");
        rt_ticks += berry_hal_aarch64::timer::cntvct() - t0;
    }
    let (hits, miss, direct) = (
        crate::telemetry::ipc_fast_hits(),
        crate::telemetry::ipc_fast_misses(),
        crate::telemetry::ipc_direct_total(),
    );
    assert!(hits >= FAST_ROUNDS - 2,
            "fa_ping: fastpath 命中须 ≥N-2（实跑 {}）", hits);
    assert!(miss >= 1, "fa_ping: slowpath 须有走数（pong 回件时 ping Ready 未睡）");
    assert!(direct >= FAST_ROUNDS - 2,
            "fa_ping: 直切须 ≥N-2（32>16 每轮必切；实跑 {}）", direct);
    let avg_ticks = rt_ticks / FAST_ROUNDS;
    let ref_cyc = avg_ticks * 1_000_000_000 / freq; // 1GHz 参考 cycle（数值 = ns）
    berry_hal_aarch64::uart_puts(b"F FAST ok\n  rt_ticks=");
    berry_hal_aarch64::uart_put_dec(avg_ticks);
    berry_hal_aarch64::uart_puts(b" ref_cyc=");
    berry_hal_aarch64::uart_put_dec(ref_cyc);
    berry_hal_aarch64::uart_puts(b" hits=");
    berry_hal_aarch64::uart_put_dec(hits);
    berry_hal_aarch64::uart_puts(b" miss=");
    berry_hal_aarch64::uart_put_dec(miss);
    berry_hal_aarch64::uart_puts(b" direct=");
    berry_hal_aarch64::uart_put_dec(direct);
    berry_hal_aarch64::uart_puts(b"\n");
    berry_sched::exit_current()
}

/// svc_tag（4-5 前置自证，prio 0）：私有表 tag 任务——两断言判别
/// `caller_table` 的 EL1 tag 路由：① 自己表 handle poll 空队列 → -EAGAIN
/// （表内有效；-EBADF 即误路由共享表）；② 共享表真 handle（fa B 端）→
/// -EBADF（自己表查无；-EAGAIN 即还在走共享表）。poll-only 不睡，验完即
/// exit（断言炸 = KERNEL PANIC → 后续标记缺失 = smoke FAIL，fail loud）。
extern "C" fn task_svc_tag(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::NR_OBJECT_WAIT_ONE;
    let h_own = arg as u32;
    let h_shared = (arg >> 32) as u32;
    let mut observed = 0u64;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_own as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
        -11, // -EAGAIN：自己表 handle 有效、队列空（误路由共享表则 -EBADF=-9）
        "svc_tag: 自己表 handle poll 空队列须 -EAGAIN"
    );
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_shared as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
        -9, // -EBADF：共享表 handle 在自己表查无（返 -EAGAIN 即还在走共享表）
        "svc_tag: 共享表 handle 须 -EBADF（tag 路由活证）"
    );
    berry_hal_aarch64::uart_puts(b"TAGTAB ok\n");
    berry_sched::exit_current()
}

// ---- 4-6 B 半：AI 最小演示（boot spawn 块见 main 尾段；设计 =
// `文档/4-6出口演示设计草案.md` B 半管线：遥测 → advisor → 注入 → 执法活证）----

/// bench 完成事件 E_done 的用户位（4-2 位面纪律：Event 用户位 1<<32 起编、
/// 与既有全部等待位零碰撞——1<<34 仅 ktest 瞬态用过（ktest 在 sched::init 前
/// 跑、运行期不在场），且本事件是独立 ObjRef，位空间互不干扰）。
const AI_EV_DONE_BIT: u64 = 1 << 34;
/// probe 放行事件 E_rel 的用户位（1<<36 全库未用）。
const AI_EV_REL_BIT: u64 = 1 << 36;

/// ai_demo ↔ ai_probe 握手标志：demo bind 注入表后、睡 E_rel 前置位——probe
/// 只在该窗口内做 clamp 断言（E_done 等待期 demo 同样在队但 eff=0，flag 区分
/// 两窗）。单核 v1 Relaxed 足够（读写都在单核串行语境）。
static AI_BOUND: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// 最近一轮 bench 的均轮 RT（CNTVCT ticks）：pinger 收尾写、demo 醒来读——
/// 单核串行天然有序（先写后 signal、醒来必读到本轮值）。
static AI_RT_LAST: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// bench 轮数（fa 128 的一半：两轮 + 探针窗都压在秒级虚拟时间内）。
const AI_ROUNDS: u64 = 64;

/// ai_demo 上下文（boot 构造、leak 常驻；spawn arg 单指针携带）。
struct AiDemoCtx {
    /// demo 表：E_done handle（bench 完成等待）。
    h_done: u32,
    /// demo 表：E_rel handle（probe 放行等待）。
    h_rel: u32,
    /// pinger arg（低 32 = bench 表通道 A 端、高 32 = bench 表 E_done）。
    h_ping_arg: usize,
    /// ponger arg（bench 表通道 B 端）。
    h_pong_arg: usize,
    /// bench 对共享表 tag（注入表合法项 key）。
    tag_p: usize,
    /// demo 自己的 tag（注入表越界探针项 key）。
    tag_d: usize,
}

/// 遥测摘要 dump（管线第 1 段）：固定 `tele:` 前缀、`k=v` 空格分隔——host 侧
/// `工具链/advisor.py` 的输入面（F6 第一级 schema：切换分型 + 唤醒分型，环内
/// 样本口径）。只读既有环（total/span/last/wake_*），不碰热路径（读点在 demo
/// 任务里，遥测写入的零分配纪律不约束消费侧）。
fn tele_dump() {
    let (mut y, mut p, mut e, mut b) = (0u64, 0u64, 0u64, 0u64);
    for i in 0..crate::telemetry::span() {
        match crate::telemetry::last(i).expect("span 内必有").kind {
            0 => y += 1,
            1 => p += 1,
            2 => e += 1,
            _ => b += 1,
        }
    }
    berry_hal_aarch64::uart_puts(b"tele: sw total=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::total());
    berry_hal_aarch64::uart_puts(b" span=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::span() as u64);
    berry_hal_aarch64::uart_puts(b" y=");
    berry_hal_aarch64::uart_put_dec(y);
    berry_hal_aarch64::uart_puts(b" p=");
    berry_hal_aarch64::uart_put_dec(p);
    berry_hal_aarch64::uart_puts(b" e=");
    berry_hal_aarch64::uart_put_dec(e);
    berry_hal_aarch64::uart_puts(b" b=");
    berry_hal_aarch64::uart_put_dec(b);
    berry_hal_aarch64::uart_puts(b"\n");
    let (mut s, mut t) = (0u64, 0u64);
    for i in 0..crate::telemetry::wake_span() {
        match crate::telemetry::wake_last(i).expect("span 内必有").reason {
            crate::telemetry::REASON_SIGNALED => s += 1,
            _ => t += 1,
        }
    }
    berry_hal_aarch64::uart_puts(b"tele: wake total=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::wake_total());
    berry_hal_aarch64::uart_puts(b" span=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::wake_span() as u64);
    berry_hal_aarch64::uart_puts(b" sig=");
    berry_hal_aarch64::uart_put_dec(s);
    berry_hal_aarch64::uart_puts(b" tmo=");
    berry_hal_aarch64::uart_put_dec(t);
    berry_hal_aarch64::uart_puts(b"\n");
}

/// ai_demo 私助：clock_read → wait_one(h, bit, now+cap) → 断言命中 → 清用户
/// 半。清位是复用前提（Event 位不自动清——不清则次轮 poll 秒中，demo 醒着
/// 高优饿死 bench）；4-2 纪律：clear 掩码用用户半全集。
fn ai_wait_ev(h: u32, bit: u64, cap_ns: u64) {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE};
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_wait_ev: clock_read");
    let mut observed = 0u64;
    // SAFETY: 同上。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h as u64, bit, now + cap_ns, pobs, 0, 0),
        0,
        "ai_wait_ev: wait（断链超时 -ETIMEDOUT 即 FAIL，fail loud）"
    );
    assert!(observed & bit != 0, "ai_wait_ev: observed 须含请求位");
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h as u64, crate::handle::SIGNAL_USER_MASK, 0),
        0,
        "ai_wait_ev: 清用户半"
    );
}

/// ai_demo 私助：spawn bench 对（tag_P 表，fa 同序——pong 先入队先睡等，ping
/// 首 send 走快路）。**运行期 spawn 须 IRQ 屏蔽**：boot 期 spawn 天然屏蔽，
/// 运行期 runqueue 链接中途被 tick 抢占 = 半链接表被 pick_next 走——手动包络。
fn ai_bench_round(ctx: &AiDemoCtx) {
    berry_hal_aarch64::disable_irq();
    berry_sched::spawn_with_tag(task_ai_pong, ctx.h_pong_arg, b"ai_pong", 32, ctx.tag_p);
    berry_sched::spawn_with_tag(task_ai_ping, ctx.h_ping_arg, b"ai_ping", 16, ctx.tag_p);
    berry_hal_aarch64::enable_irq();
}

/// ai_demo（4-6 B 半主角，prio 0，tag_D 私有表）：F1 管线一次走完——①睡
/// 2 slice（boot 期任务先跑、遥测积累）→ ②tele dump → ③bench 轮 1（无表
/// 基线 rt1）→ ④bind 注入表（合法 +8 / 越界 +200 探针；IRQ 屏蔽窗写点）→
/// ⑤睡 E_rel（bind 后睡入 SLEEPQ，PI 尾趟 refresh_effective 经 weight_of 同
/// clamp 把本任务 eff 刷到 0+64——probe 的断言窗口，醒来由 probe signal 放行）
/// → ⑥bench 轮 2（表在：16+8/32+8=24/40 同层相对序保持；demo 照睡不抢 CPU）
/// → ⑦unbind → ⑧开销断言（F9① 最小兑现：rt2 ≤ 2×rt1，TCG 噪声容差）→
/// ⑨打 `AIDEMO ok`（smoke 第 22 标记）→ exit。
extern "C" fn task_ai_demo(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_THREAD_SLEEP};
    // SAFETY: ctx 由 boot Box::leak 常驻（活过本任务全程）。
    let ctx = unsafe { &*(arg as *const AiDemoCtx) };
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_demo: clock_read");
    // ① 睡 2 slice（bench 对 prio 16/32 严格优先本就先完成，此睡更稳）。
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 2 * slice_ns, 0, 0),
        0,
        "ai_demo: thread_sleep 2 slice"
    );
    // ② 遥测 dump（advisor 输入面）。
    tele_dump();
    // ③ bench 轮 1（无表基线）。
    ai_bench_round(ctx);
    ai_wait_ev(ctx.h_done, AI_EV_DONE_BIT, 100 * 1_000_000_000);
    let rt1 = AI_RT_LAST.load(core::sync::atomic::Ordering::Relaxed);
    // ④ bind 注入表（advisor.py 产出快照——见 boot 4-6 段与 工具链/advisor.py；
    // 表运行期 Box::leak：key 是运行期表指针）。越界项 = 拒收面探针：内核在
    // apply/weight_of 双点夹到 ±64（F9② 单调收窄——夹取是唯一方向，无提权
    // 路径），机器断言交 probe。
    let tbl: &'static [berry_sched::policy::Entry] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new([
            berry_sched::policy::Entry {
                task_key: ctx.tag_p as u64,
                directive: berry_sched::policy::Directive {
                    placement: 0, weight: 8, epp: None,
                },
            },
            berry_sched::policy::Entry {
                task_key: ctx.tag_d as u64,
                directive: berry_sched::policy::Directive {
                    placement: 0, weight: 200, epp: None,
                },
            },
        ]));
    // 写点 IRQ 屏蔽（policy 契约：bind/unbind 仅屏蔽窗——决策路径只读）。
    berry_hal_aarch64::disable_irq();
    berry_sched::policy::bind(tbl);
    berry_hal_aarch64::enable_irq();
    // 拒收 trace（demo 侧打——apply 是 IRQ 热路径不得加打印，F9①；机器断言
    // 交 probe 读字段）。
    berry_hal_aarch64::uart_puts(b"  policy: reject w=+200 -> clamp +64 (envelope +-64)\n");
    // ⑤ 睡 E_rel：probe 断言窗口（醒来由 probe signal 放行）。
    AI_BOUND.store(true, core::sync::atomic::Ordering::Relaxed);
    ai_wait_ev(ctx.h_rel, AI_EV_REL_BIT, 100 * 1_000_000_000);
    // ⑥ bench 轮 2（表在）。
    ai_bench_round(ctx);
    ai_wait_ev(ctx.h_done, AI_EV_DONE_BIT, 100 * 1_000_000_000);
    let rt2 = AI_RT_LAST.load(core::sync::atomic::Ordering::Relaxed);
    // ⑦ unbind（恢复接口缺席态）。
    berry_hal_aarch64::disable_irq();
    berry_sched::policy::unbind();
    berry_hal_aarch64::enable_irq();
    // ⑧ 开销断言（F9① 最小兑现：注入表在场时 bench RT 须同量级——热路径增
    // 量 = 2 条目线性查表 + 权重算术，TCG 下远低于噪声；容差 2x 是噪声界非
    // 精度断言，-icount 精确版/真机 p99 随 FAST 线留 G 卡复测）。
    assert!(rt1 > 0, "ai_demo: rt1 须非零");
    assert!(
        rt2 <= rt1 * 2,
        "ai_demo: 注入后 bench RT 须同量级（rt1={} rt2={}，容差 2x）", rt1, rt2
    );
    // ⑨ 收官标记（smoke 第 22）。
    berry_hal_aarch64::uart_puts(b"J AIDEMO ok: rt1=");
    berry_hal_aarch64::uart_put_dec(rt1);
    berry_hal_aarch64::uart_puts(b" rt2=");
    berry_hal_aarch64::uart_put_dec(rt2);
    berry_hal_aarch64::uart_puts(b" clamp=64\n");
    berry_sched::exit_current()
}

/// ai_probe（4-6 B 半验收司机，prio 0，挂第三张私有表）：1 slice 步进重试
/// （≤64 次）等 `AI_BOUND` 窗口内的 ai_demo 在队 → 断言其 `prio_effective
/// == 64` 且 `wait_signals == AI_EV_REL_BIT`（raw 指令 +200、在队字段 64
/// ——内核 clamp 执法的机器活证）→ signal E_rel 放行 → exit。步进重试而非
/// 一次长睡：boot 期任务次序不定，「标记不得建立在时序竞速上」——条件
/// （flag && 在队）满足才前进，窗口由 demo 的 100s 帽保持敞开、预算 64
/// slice ≈ 6.4s 远超最慢轮转。**私有表纪律（4-5 同款，本步实踩）**：probe
/// 是长命睡眠内核任务，睡共享表会被 pi 窗错捐 eff 32（冻结随行、严格优先
/// 反噬 prio-0 轮转）——自己的唤醒周期反而把 ai_demo 的轮次推过自身重试
/// 预算（bisect 实测：demo 第二轮 ~830ms 才到、8×1 slice 预算 ~816ms 先
/// 尽，panic「时序未达」）。挂私有表断根：pi 受捐反查只看自己的表。另
/// for_each_sleeper 契约须 IRQ 屏蔽窗内调用（tick 超时摘队会动睡眠链接，
/// 遍历中接线即扬队）——probe 任务体经 SVC eret 后 IRQ 已解蔽，扫描窗手动
/// 包络（it_probe 未包络属侥幸窗口小；本 probe 扫描最多 64 次不赌）。
extern "C" fn task_ai_probe(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_rel = arg as u32;
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    let mut saw = false;
    for _ in 0..64u64 {
        if AI_BOUND.load(core::sync::atomic::Ordering::Relaxed) {
            berry_hal_aarch64::disable_irq();
            berry_sched::for_each_sleeper(|t| {
                match t.name {
                    b"ai_demo" => {
                        saw = true;
                        assert_eq!(t.wait_signals, AI_EV_REL_BIT,
                                   "ai_probe: demo 须睡 E_rel 位（AI_BOUND 窗口）");
                        assert_eq!(t.prio_effective, 64,
                                   "ai_probe: demo eff 须为 clamp 后的 64（+200 越界探针）");
                    }
                    _ => {}
                }
            });
            berry_hal_aarch64::enable_irq();
            if saw {
                break;
            }
        }
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_probe: clock_read");
        assert_eq!(
            svc4(NR_THREAD_SLEEP as u64, now + slice_ns, 0, 0),
            0,
            "ai_probe: thread_sleep 1 slice（步进重试）"
        );
    }
    assert!(saw, "ai_probe: 64 次重试未见 bind 窗口内的 ai_demo 在队（时序未达）");
    // 放行：signal E_rel（probe 私有表 handle——与 demo 表同一 ObjRef，跨表互通）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_rel as u64, 0, AI_EV_REL_BIT),
        0,
        "ai_probe: signal E_rel 放行"
    );
    berry_sched::exit_current()
}

/// ai_pong（prio 32，bench 对 B 端）：fa_pong 同构、AI_ROUNDS 轮、不触
/// ipc_fast 三计数器断言（demo bench 与 fa 口径分离——fa 断言虽已先过，别混
/// 进 demo 轮次的计数）。
extern "C" fn task_ai_pong(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let hb = arg as u32;
    let mut buf = [0u8; 8];
    for _ in 0..AI_ROUNDS {
        let mut now: u64 = 0;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pnow = core::ptr::addr_of_mut!(now) as u64;
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_pong: clock_read");
        let mut observed = 0u64;
        // SAFETY: 同上。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // 10s 帽兜底（fa_pong 同款：断链则 -ETIMEDOUT 断言失败 = smoke FAIL）。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, hb as u64, SIGNAL_READABLE,
                 now + 10_000_000_000, pobs, 0, 0),
            0,
            "ai_pong: wait READABLE"
        );
        assert!(observed & SIGNAL_READABLE != 0, "ai_pong: observed 须含 READABLE");
        let (mut n, mut nh): (u64, u64) = (8, 0);
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, hb as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "ai_pong: recv"
        );
        assert!(buf[..3] == *b"AIP", "ai_pong: 载荷须 AIP");
        buf[..3].copy_from_slice(b"AIR");
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hb as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "ai_pong: 回件"
        );
    }
    berry_sched::exit_current()
}

/// ai_ping（prio 16，bench 对 A 端）：fa_ping 同构 + 收尾写 AI_RT_LAST →
/// signal E_done（bench 表 handle）通知 demo。RT 计量同 fa（CNTVCT 差累计/
/// 轮均）——bind 前后两轮同法，差分即 F9① 断言对象（先写 RT 再 signal：单
/// 核串行，demo 醒来读到的必是本轮值）。
extern "C" fn task_ai_ping(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE};
    let ha = arg as u32;
    let h_done = (arg >> 32) as u32;
    let mut buf = [0u8; 8];
    let mut rt_ticks: u64 = 0;
    for _ in 0..AI_ROUNDS {
        buf[..4].copy_from_slice(b"AIP0");
        let t0 = berry_hal_aarch64::timer::cntvct();
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, ha as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "ai_ping: send（pong 在睡等 READABLE）"
        );
        let mut observed = 0u64;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, ha as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
            0,
            "ai_ping: wait poll 命中（断链 -EAGAIN 即 FAIL）"
        );
        let (mut n, mut nh): (u64, u64) = (8, 0);
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, ha as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "ai_ping: recv 回件"
        );
        assert!(buf[..3] == *b"AIR", "ai_ping: 载荷须 AIR");
        rt_ticks += berry_hal_aarch64::timer::cntvct() - t0;
    }
    AI_RT_LAST.store(rt_ticks / AI_ROUNDS, core::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_done as u64, 0, AI_EV_DONE_BIT),
        0,
        "ai_ping: signal E_done"
    );
    berry_sched::exit_current()
}
