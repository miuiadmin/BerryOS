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

use heap::SlabHeap;

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
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配。
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
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

    // 3-2：good 的 root Vmar 进**它自己的表** = 首能力（能力模型 §5「第一个
    // 能力从哪来」——用户拿自己的地址空间根，MAP|MANAGE|DUPLICATE）。handle
    // 作 spawn arg → 入口 x0 → good 的 3-2 验收链经真 syscall map 读写自己的
    // VMO（全链落在 good 自己的表里——per-process 拆分后其余进程看不见）。
    let good_vmar = up_good.vmar.take().expect("main: good vmar");
    let good_vmar_h = ht_good
        .insert(KernelObj::Vmar(good_vmar), RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE)
        .expect("main: 插 root Vmar 进 good 表");

    berry_sched::spawn_user(user::USER_CODE_VA, good_vmar_h as usize, user::USER_STACK_TOP,
                          b"user_good", up_good.ttbr0_pa, up_good.asid, tag_good);
    berry_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_rogue_ptr", up_rogue_ptr.ttbr0_pa, up_rogue_ptr.asid, tag_rogue);
    berry_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_abuser", up_abuser.ttbr0_pa, up_abuser.asid, tag_abuser);
    // 3-3/3-4 验收：spawner 经真 syscall 创建第二个进程；process_start 的
    // bootstrap 迁移把「礼物 VMO」handle 送进子进程表（子入口 x0 收到后读出
    // 魔数自证——跨进程能力流动，见 user.rs spawner 模板注释）。
    berry_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
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

/// 4-0 waker：轮询 `wake_matching(SLEEP_OBJ, 1<<33)` 直到摘到 sleeper（返回
/// 1）。未挂上时返回 0 且零副作用——单核屏蔽窗原子保证无丢唤醒：要么本循环
/// 先扫（0，sleeper 稍后挂上，下轮摘到）、要么 sleeper 已挂（本轮即摘）。轮
/// 询体 IRQ 基本解蔽 → tick 正常抢占 → sleeper 得以上 CPU 挂队。命中即收工
/// 退出。（置位集与 sleeper 的请求位同步收窄到用户位 1<<33——理由见路径 2
/// 注释。）
extern "C" fn task_waker(_arg: usize) -> ! {
    loop {
        berry_hal_aarch64::disable_irq();
        let n = berry_sched::wake_matching(&SLEEP_OBJ as *const u8 as usize, 1 << 33);
        berry_hal_aarch64::enable_irq();
        if n == 1 {
            break;
        }
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
