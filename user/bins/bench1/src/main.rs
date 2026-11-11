//! bench1 —— B-1 EL0 用户态基准（docs/基准与压力测试设计.md §8；stdhello
//! 同链：musl target + berry-libc shim + berry-sys 直调）。
//!
//! 四项基准（单进程自足——vmo/channel/event 经 syscall 自造，零新管道）：
//!   `sys_null`      clock_read 往返 = SVC 陷阱 + 派发 + 最薄 handler 全链
//!                   下界，iters 4096（t0/t1 夹 N 次调用）；
//!   `sys_vmo_rw4k`  syscall 面 4KiB vmo_write+read（查表 + 权利 + 用户缓冲
//!                   拷贝全链），自建 VMO commit 后循环，iters 128；
//!   `ipc_enqueue8`  自持双端 channel send×8 填满（慢路入队——无等待者）
//!                   + recv×8 排空（对端收，S-2 xh_chan 同拓扑），载荷 8B
//!                   （S-2 XH_MSG 同值——最小有意义载荷），iters 128；
//!   `evt_signal`    object_signal（用户位）+ object_wait_one poll 命中的
//!                   操作对，iters 1024。
//!
//! **输出纪律（bsh collect 10s 活动帽——每收一包刷新，静默 >10s 即断）**：
//! 每项跑完**立即** println 一行 `BENCH1,<name>,<iters>,<cyc_per_op>`（~35B
//! ≤192B 单消息原子），增量输出——禁止四项全毕再批量打；spawn(~4.5s TCG) +
//! 首项 sys_null 4096 次 SVC 的首行间隔**估算** <10s（4.5s 系 U1 时代旧测
//! 外推——验收首跑实测校准；公约 7 kill criteria 触发时削 4096→2048，
//! 设计预授权）。
//!
//! **cyc 数值口径 = per-iteration，非全项同一「单 syscall」**：sys_null=1
//! syscall/迭代；sys_vmo_rw4k、evt_signal=操作对（2 syscall）/迭代；
//! ipc_enqueue8=整轮（填满+排空共 16 syscall）/轮——**跨项直读对比须先
//! 折算**（如 sys_null vs ipc_enqueue8 差 16 倍口径）；基线只做自名相对
//! 比较，无对拍错误。
//!
//! **计时口径（公约 5）**：CNTVCT 周期直读 `mrs cntvct_el0`（镜像
//! arch/aarch64/src/timer.rs `cntvct()` 形态）；**不走 clock_read ns**——
//! 那是 CNTFRQ 名义换算，TCG 半率下失真 2×，换算舍入还污染被测量。边界：
//! EL0 读 CNTVCT_EL0 需 CNTKCTL_EL1.EL0VCTEN=1——复位 0 即 EC=0x18 陷阱
//! 静默杀（fault 杀码 0 无 EOF），内核 timer init 显式开位（B-1 前置修复，
//! Linux 同款 enable；首跑实踩详见设计 §8 落地注记）；EL2 场另需
//! CNTHCTL_EL2.EL0VCTEN——run-el2 裸跑无 rootfs 不触本程序（文档化边界）。
//!
//! **evt_signal 用户位 = 1<<32**：Event 为本进程私有（event_create 自造、
//! 仅自己 wait、随进程销毁）——无内核等待者、无跨任务竞争，ktest 瞬态位
//! 先例（ktest.rs 局部位不登记 USER_SIGNAL_BITS 同理；登记义务约束内核任务
//! 等待面/跨任务碰撞面）；set 后位恒在 → 每次 wait poll 恒命中（量
//! signal+wait 操作对成本合法，scout 裁决）。勿占 R 线 rt_probe 预留 1<<40。
//!
//! 正常 main 返回 = 退出链（libc exit 族 EOF 先行 → bsh collect 终点）；
//! setup/中途失败 = eprintln 诊断（fd2 与 stdout 同通道）+ `exit(1)`——
//! bsh 侧 code==0 断言 loud 失败。cyc 数值不做任何自断言（公约 2——统计
//! 归统计，对拍归 bench.py --el0 L1 宽松因子）。

use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_event_create, berry_handle_close, berry_object_signal, berry_object_wait_one,
    berry_thread_yield, berry_vmo_commit, berry_vmo_create, berry_vmo_read, berry_vmo_write, Handle, Status,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC,
};

// 链接种子（stdhello 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的
// crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

// —— iters 定值（设计 §8 表；bsh bench1_selftest 的字面常量断言与此镜像，
//    bench.py _iter_table_el0 第三镜——改值三处同步）——
const SYS_NULL_ITERS: u64 = 4096;
const VMO_RW4K_ITERS: u64 = 128;
const IPC_ENQUEUE8_ITERS: u64 = 128;
const EVT_SIGNAL_ITERS: u64 = 1024;

/// ipc 载荷与收发缓冲（8B = S-2 XH_MSG 同值——最小有意义载荷）。
const IPC_MSG: [u8; 8] = [0x42; 8];

/// 失败面：诊断 + 退出真码 1（bsh 侧 code==0 / 标记缺失 loud 兜底）。
fn bail(what: &str, s: Status) -> ! {
    eprintln!("bench1: {} failed status={}", what, s);
    std::process::exit(1)
}

/// CNTVCT 周期直读（公约 5——不走 clock_read ns 名义换算）。
#[inline(always)]
fn cntvct() -> u64 {
    let v: u64;
    // SAFETY: 只读计数器（EL0 可读性边界见头注）。
    unsafe {
        core::arch::asm!(
            "mrs {0}, cntvct_el0",
            out(reg) v,
            options(nostack, preserves_flags)
        );
    }
    v
}

fn main() {
    // 1.1/S1：0x3F 用户态活证（SVC 类/切换锁舞蹈走真链；输出面零变化——
    // B1 形状不变，yield 失败即 bail 退出非 0）。
    let s = berry_thread_yield();
    if s != 0 {
        bail("thread_yield", s);
    }
    bench_sys_null();
    bench_vmo_rw4k();
    bench_ipc_enqueue8();
    bench_evt_signal();
}

/// sys_null：clock_read 往返全链下界（SVC 陷阱 + 派发 + 最薄 handler）。
fn bench_sys_null() {
    let mut t = 0u64;
    let mut sink = 0u64;
    let t0 = cntvct();
    for _ in 0..SYS_NULL_ITERS {
        let s = berry_clock_read(CLOCK_MONOTONIC, &mut t);
        if s != 0 {
            bail("clock_read", s);
        }
        sink = sink.wrapping_add(t);
    }
    let t1 = cntvct();
    std::hint::black_box(&sink);
    println!("BENCH1,sys_null,{},{}", SYS_NULL_ITERS, (t1 - t0) / SYS_NULL_ITERS);
}

/// sys_vmo_rw4k：4KiB vmo_write+read 往返（查表 + 权利 + 用户缓冲拷贝）。
fn bench_vmo_rw4k() {
    let mut h: Handle = 0;
    let s = berry_vmo_create(4096, 0, &mut h);
    if s != 0 {
        bail("vmo_create", s);
    }
    // 未提交页读写 = 错误码——commit 前置（S-2 vmo 边界同款在册语义）。
    let s = berry_vmo_commit(h, 0, 4096);
    if s != 0 {
        bail("vmo_commit", s);
    }
    let wbuf = [0x5au8; 4096];
    let mut rbuf = [0u8; 4096];
    let t0 = cntvct();
    for _ in 0..VMO_RW4K_ITERS {
        let s = berry_vmo_write(h, wbuf.as_ptr(), 0, 4096);
        if s != 0 {
            bail("vmo_write", s);
        }
        let s = berry_vmo_read(h, rbuf.as_mut_ptr(), 0, 4096);
        if s != 0 {
            bail("vmo_read", s);
        }
    }
    let t1 = cntvct();
    // 轻量一致性抽验（末轮读回——基准非正确性测试，但恒错立现）。
    if rbuf[0] != 0x5a || rbuf[4095] != 0x5a {
        eprintln!("bench1: vmo readback mismatch");
        std::process::exit(1);
    }
    let _ = berry_handle_close(h);
    std::hint::black_box(&rbuf);
    println!(
        "BENCH1,sys_vmo_rw4k,{},{}",
        VMO_RW4K_ITERS,
        (t1 - t0) / VMO_RW4K_ITERS
    );
}

/// ipc_enqueue8：自持双端 send×8 填满（慢路入队）+ 对端 recv×8 排空
/// （S-2 xh_chan 同拓扑：send 走 hc0、recv 走 hc1；无等待者 = 纯
/// enqueue/dequeue 成本）。一轮 = 填满 + 排空，cyc_per_op 按轮计
/// （=16 syscall/轮——跨项对比先折算，见头注口径段）。
fn bench_ipc_enqueue8() {
    let mut a: Handle = 0;
    let mut b: Handle = 0;
    let s = berry_channel_create(0, &mut a, &mut b);
    if s != 0 {
        bail("channel_create", s);
    }
    let mut rbuf = [0u8; 8];
    // 深度钉板探针（未计时，ktest H4/S-2 chan_full 同款）：send×8 全成 +
    // 第 9 发恰 -EBUSY——内层字面量 8 与 kernel `CHAN_DEPTH=8` 的耦合在此
    // loud 执法：深度调小 ⇒ 第 8 发即拒、调大 ⇒ 第 9 发不再拒，都 bail，
    // 不许「满队 enqueue」语义静默变义（iters 三镜只锁 4096/128 不锁深度）。
    // 探针末尾排水recv×8 归零后才进计时循环。
    for i in 0..9 {
        let s = berry_channel_send(a, IPC_MSG.as_ptr(), IPC_MSG.len(), core::ptr::null(), 0);
        if i < 8 {
            if s != 0 {
                bail("channel_send(depth probe)", s);
            }
        } else if s != BERRY_ERR_BUSY {
            bail("channel_send(depth probe: 9th want -EBUSY)", s);
        }
    }
    for _ in 0..8 {
        let mut nb = rbuf.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(
            b,
            rbuf.as_mut_ptr(),
            &mut nb,
            core::ptr::null_mut(),
            &mut nh,
        );
        if s != 0 || nb != IPC_MSG.len() {
            bail("channel_recv(depth probe)", s);
        }
    }
    let t0 = cntvct();
    for _ in 0..IPC_ENQUEUE8_ITERS {
        for _ in 0..8 {
            let s = berry_channel_send(a, IPC_MSG.as_ptr(), IPC_MSG.len(), core::ptr::null(), 0);
            if s != 0 {
                bail("channel_send", s);
            }
        }
        for _ in 0..8 {
            let mut nb = rbuf.len();
            let mut nh = 0usize;
            let s = berry_channel_recv(
                b,
                rbuf.as_mut_ptr(),
                &mut nb,
                core::ptr::null_mut(),
                &mut nh,
            );
            if s != 0 || nb != IPC_MSG.len() {
                bail("channel_recv", s);
            }
        }
    }
    let t1 = cntvct();
    let _ = berry_handle_close(a);
    let _ = berry_handle_close(b);
    std::hint::black_box(&rbuf);
    println!(
        "BENCH1,ipc_enqueue8,{},{}",
        IPC_ENQUEUE8_ITERS,
        (t1 - t0) / IPC_ENQUEUE8_ITERS
    );
}

/// evt_signal：object_signal（用户位 1<<32）+ object_wait_one poll 命中。
fn bench_evt_signal() {
    const BIT: u64 = 1 << 32; // 用户半（进程私有——见头注，不登记 USER_SIGNAL_BITS）
    let mut h: Handle = 0;
    let s = berry_event_create(0, &mut h);
    if s != 0 {
        bail("event_create", s);
    }
    let mut obs = 0u64;
    let t0 = cntvct();
    for _ in 0..EVT_SIGNAL_ITERS {
        let s = berry_object_signal(h, 0, BIT);
        if s != 0 {
            bail("object_signal", s);
        }
        // deadline=0 纯 poll；位置位后恒命中（未命中 -EAGAIN 即 bail）。
        let s = berry_object_wait_one(h, BIT, 0, &mut obs);
        if s != 0 {
            bail("object_wait_one", s);
        }
    }
    let t1 = cntvct();
    // observed 回写全满足集（ktest 同款形状）——位不在 = poll 命中语义破。
    if obs & BIT == 0 {
        eprintln!("bench1: evt observed={} want bit 1<<32", obs);
        std::process::exit(1);
    }
    let _ = berry_handle_close(h);
    std::hint::black_box(&obs);
    println!(
        "BENCH1,evt_signal,{},{}",
        EVT_SIGNAL_ITERS,
        (t1 - t0) / EVT_SIGNAL_ITERS
    );
}
